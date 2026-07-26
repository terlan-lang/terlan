use std::fs;
use std::io;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore};

use super::test_support::{ChunkedReader, FailingReader};
use super::{
    accept_http1_tcp_handler, build_http_fairness_replay_seed, finish_http1_tcp_handler,
    handle_http1_in_memory_exchange, handle_http1_tcp_exchange, incomplete_http1_request_error,
    parse_http1_request_headers, poll_http1_tcp_exchange, poll_http1_tcp_keep_alive_exchange,
    poll_http1_tls_tcp_exchange, poll_or_park_http1_tcp_exchange,
    poll_or_park_http1_tls_tcp_exchange_with_connection, read_http1_request, read_http1_response,
    render_http_template_response, stream_http_request_body_for_dispatch, write_http1_response,
    VmHttpQueue, VmHttpQueueMetrics, VmHttpTcpActorPoll, VmHttpTcpHandler, VmHttpTcpPoll,
    VmHttpTcpRequestBuffer, VmHttpTcpServer, VmHttpTcpServerInfo, VmHttpTcpServerPoll,
    VmHttpTemplateResponse, VmTcpReadStream,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome},
    tcp::{VmTcpListenerInfo, VmTcpRuntime, VmTcpStream},
    tcp_scheduler::apply_tcp_wakeups,
    tls::{VmTlsMode, VmTlsPlan, VmTlsRuntime, VmTlsTcpServerStream, VmTlsTransportMode},
};
use crate::support::test_fs;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn arithmetic_response_from_query(query: &str) -> Result<http::Response<String>, String> {
    let response = match (query_int(query, "a"), query_int(query, "b")) {
        (Ok(left), Ok(right)) => http::Response::builder()
            .status(200)
            .body((left + right).to_string()),
        (Err(message), _) | (_, Err(message)) => {
            http::Response::builder().status(400).body(message)
        }
    };
    response.map_err(|error| error.to_string())
}

fn query_int(query: &str, name: &str) -> Result<i64, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
        .ok_or_else(|| "missing query integer".to_string())?
        .parse::<i64>()
        .map_err(|_| "invalid query integer".to_string())
}

fn plain_tls_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Plain,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: None,
        key_path: None,
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

fn manual_tls_plan() -> VmTlsPlan {
    VmTlsPlan {
        mode: VmTlsMode::Manual,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert_path: Some("/cert.pem".to_string()),
        key_path: Some("/key.pem".to_string()),
        passphrase_env: None,
        ca_path: None,
        server_name: None,
        trust_local: None,
    }
}

fn manual_tls_plan_with_paths(cert_path: String, key_path: String) -> VmTlsPlan {
    VmTlsPlan {
        cert_path: Some(cert_path),
        key_path: Some(key_path),
        ..manual_tls_plan()
    }
}

fn write_http_tls_cert_pair(name: &str) -> (std::path::PathBuf, String, String, Vec<u8>) {
    let dir = test_fs::temp_path("vm_http_tls", name);
    fs::create_dir_all(&dir).expect("create TLS fixture dir");
    let generated =
        generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    let cert_der = generated.cert.der().as_ref().to_vec();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    fs::write(&cert_path, generated.cert.pem()).expect("write cert");
    fs::write(&key_path, generated.key_pair.serialize_pem()).expect("write key");
    (
        dir,
        cert_path.to_string_lossy().to_string(),
        key_path.to_string_lossy().to_string(),
        cert_der,
    )
}

fn http_tls_client_for_cert(cert_der: Vec<u8>) -> ClientConnection {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(cert_der))
        .expect("root cert should install");
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
    ClientConnection::new(
        Arc::new(config),
        ServerName::try_from("localhost").expect("server name"),
    )
    .expect("client connection")
}

fn flush_http_client_tls_to_tcp(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
) {
    let mut bytes = Vec::new();
    client.write_tls(&mut bytes).expect("client writes TLS");
    if !bytes.is_empty() {
        tcp.send(client_stream, bytes)
            .expect("client sends TLS over VM TCP");
    }
}

fn pump_http_tcp_to_tls_client(
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    client: &mut ClientConnection,
) {
    while let Some(bytes) = tcp
        .receive(client_stream, 16 * 1024)
        .expect("client receives TLS")
    {
        let consumed = client
            .read_tls(&mut Cursor::new(bytes.as_slice()))
            .expect("client reads server TLS bytes");
        assert_eq!(consumed, bytes.len());
        client
            .process_new_packets()
            .expect("client processes TLS packets");
    }
}

fn complete_http_tls_handshake(
    client: &mut ClientConnection,
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    server: &mut VmTlsTcpServerStream,
) {
    flush_http_client_tls_to_tcp(client, tcp, client_stream);
    for _ in 0..10 {
        let _ = server.poll(tcp).expect("server polls TLS over VM TCP");
        pump_http_tcp_to_tls_client(tcp, client_stream, client);
        flush_http_client_tls_to_tcp(client, tcp, client_stream);
        if !client.is_handshaking() && !server.inspect().handshaking {
            return;
        }
    }
    panic!("HTTP TLS handshake did not complete");
}

fn complete_retained_http_tls_server_handshake(
    processes: &mut VmProcessTable,
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    client: &mut ClientConnection,
    server: &mut VmHttpTcpServer,
) -> VmProcessId {
    let handler_process = server.handlers[0].process;
    complete_http_tls_handshake(
        client,
        tcp,
        client_stream,
        server.handlers[0]
            .tls_stream
            .as_mut()
            .expect("handler TLS stream"),
    );
    processes
        .get_mut(handler_process)
        .expect("handler process")
        .wake();
    handler_process
}

fn read_retained_http_tls_response(
    tcp: &mut VmTcpRuntime,
    client_stream: VmTcpStream,
    client: &mut ClientConnection,
) -> String {
    pump_http_tcp_to_tls_client(tcp, client_stream, client);
    let mut response = [0; 256];
    let read = client
        .reader()
        .read(&mut response)
        .expect("read decrypted response");
    std::str::from_utf8(&response[..read])
        .expect("response UTF-8")
        .to_string()
}

#[test]
fn vm_http_queue_rejects_zero_capacity() {
    let error = match VmHttpQueue::<i32>::new(0) {
        Ok(_) => panic!("zero capacity must fail"),
        Err(error) => error,
    };

    assert_eq!(error, "VM HTTP queue capacity must be greater than 0");
}

#[test]
fn vm_http_queue_preserves_fifo_order_and_metrics() {
    let queue = VmHttpQueue::new(3).expect("queue should be created");

    queue.enqueue(1).expect("first enqueue should work");
    queue.enqueue(2).expect("second enqueue should work");
    queue.enqueue(3).expect("third enqueue should work");

    assert_eq!(queue.capacity(), 3);
    assert_eq!(queue.dequeue().expect("first dequeue"), 1);
    assert_eq!(queue.dequeue().expect("second dequeue"), 2);
    assert_eq!(queue.dequeue().expect("third dequeue"), 3);
    let metrics = queue.metrics().expect("metrics should read");
    assert_eq!(metrics.current_depth, 0);
    assert_eq!(metrics.max_depth, 3);
    assert_eq!(metrics.enqueue_count, 3);
    assert_eq!(metrics.dequeue_count, 3);
}

#[test]
fn vm_http_queue_blocks_enqueue_until_consumer_frees_capacity() {
    let queue = Arc::new(VmHttpQueue::new(1).expect("queue should be created"));
    queue.enqueue(1).expect("first enqueue should fill queue");

    let producer_queue = Arc::clone(&queue);
    let producer = thread::spawn(move || {
        producer_queue
            .enqueue(2)
            .expect("second enqueue should unblock");
    });

    thread::sleep(Duration::from_millis(10));
    assert_eq!(queue.dequeue().expect("first dequeue"), 1);
    producer.join().expect("producer should finish");
    assert_eq!(queue.dequeue().expect("second dequeue"), 2);

    let metrics = queue.metrics().expect("metrics should read");
    assert_eq!(metrics.max_depth, 1);
    assert_eq!(metrics.enqueue_wait_count, 1);
    assert!(metrics.enqueue_wait_total_ns > 0);
    assert_eq!(metrics.max_parked_producers, 1);
    assert_eq!(metrics.producer_wakeup_count, 1);
    assert_eq!(metrics.parked_producers, 0);
}

#[test]
fn vm_http_queue_blocks_dequeue_until_producer_adds_item() {
    let queue = Arc::new(VmHttpQueue::new(1).expect("queue should be created"));

    let consumer_queue = Arc::clone(&queue);
    let consumer = thread::spawn(move || consumer_queue.dequeue().expect("dequeue should unblock"));

    thread::sleep(Duration::from_millis(10));
    queue.enqueue(42).expect("enqueue should wake consumer");

    assert_eq!(consumer.join().expect("consumer should finish"), 42);
    let metrics = queue.metrics().expect("metrics should read");
    assert_eq!(metrics.dequeue_wait_count, 1);
    assert!(metrics.dequeue_wait_total_ns > 0);
    assert_eq!(metrics.max_parked_consumers, 1);
    assert_eq!(metrics.consumer_wakeup_count, 1);
    assert_eq!(metrics.parked_consumers, 0);
}

#[test]
fn vm_http_fairness_replay_seed_captures_queue_and_server_counters() {
    let poll = VmHttpTcpServerPoll {
        accepted: 2,
        rejected: 0,
        spilled: 0,
        polled: 3,
        parked: 1,
        completed: 2,
        skipped_blocked: 1,
    };
    let server = VmHttpTcpServerInfo {
        listener: VmTcpListenerInfo {
            address: "127.0.0.1:8080".to_string(),
            backlog_limit: 16,
            queued_accepts: 4,
            waiting_acceptors: 0,
            closed: false,
        },
        overload: None,
        active_handlers: 5,
        next_handler_index: 2,
        accepted_total: 8,
        rejected_total: 0,
        spilled_total: 0,
        completed_total: 3,
    };
    let queue = VmHttpQueueMetrics {
        max_depth: 7,
        enqueue_wait_count: 2,
        enqueue_wait_total_ns: 500,
        ..VmHttpQueueMetrics::default()
    };

    let seed = build_http_fairness_replay_seed("socket-c8-pressure", &poll, &server, &queue)
        .expect("seed");

    assert_eq!(seed.seed_id, "socket-c8-pressure:a2:p3:k1:s1:c2:h5:q7");
    assert_eq!(seed.accepted, 2);
    assert_eq!(seed.polled, 3);
    assert_eq!(seed.parked, 1);
    assert_eq!(seed.skipped_blocked, 1);
    assert_eq!(seed.completed, 2);
    assert_eq!(seed.active_handlers, 5);
    assert_eq!(seed.next_handler_index, 2);
    assert_eq!(seed.queued_accepts, 4);
    assert_eq!(seed.max_queue_depth, 7);
}

#[test]
fn vm_http_fairness_replay_seed_rejects_empty_labels() {
    let error = build_http_fairness_replay_seed(
        "",
        &VmHttpTcpServerPoll::default(),
        &VmHttpTcpServerInfo {
            listener: VmTcpListenerInfo {
                address: "127.0.0.1:8080".to_string(),
                backlog_limit: 1,
                queued_accepts: 0,
                waiting_acceptors: 0,
                closed: false,
            },
            overload: None,
            active_handlers: 0,
            next_handler_index: 0,
            accepted_total: 0,
            rejected_total: 0,
            spilled_total: 0,
            completed_total: 0,
        },
        &VmHttpQueueMetrics {
            max_depth: 0,
            enqueue_wait_count: 0,
            enqueue_wait_total_ns: 0,
            ..VmHttpQueueMetrics::default()
        },
    )
    .expect_err("empty labels must fail");

    assert_eq!(
        error,
        "VM HTTP fairness replay seed label must not be empty"
    );
}

#[test]
fn vm_http_reads_http1_request_with_headers_query_and_body() {
    let mut wire = "POST /api/users?page=2 HTTP/1.1\r\n\
Host: localhost\r\n\
Accept: application/json\r\n\
Content-Length: 7\r\n\
\r\n\
payload"
        .as_bytes();

    let request = read_http1_request(&mut wire).expect("request should parse");

    assert_eq!(request.method().as_str(), "POST");
    assert_eq!(request.uri().path(), "/api/users");
    assert_eq!(request.uri().query(), Some("page=2"));
    assert_eq!(request.headers()["accept"], "application/json");
    assert_eq!(request.body(), "payload");
}

#[test]
fn vm_http_reads_fragmented_request_body() {
    let mut reader = ChunkedReader::new(vec![
        b"POST /chunked HTTP/1.1\r\nContent-Length: 7\r\n\r\npay".to_vec(),
        b"load".to_vec(),
    ]);

    let request = read_http1_request(&mut reader).expect("fragmented request should parse");

    assert_eq!(request.uri().path(), "/chunked");
    assert_eq!(request.body(), "payload");
}

#[test]
fn vm_http_rejects_partial_request_headers() {
    let mut wire = "GET / HTTP/1.1\r\nHost: localhost\r\n".as_bytes();

    let error = read_http1_request(&mut wire).expect_err("partial request should fail");

    assert_eq!(error, "VM HTTP request closed before headers completed");
}

#[test]
fn vm_http_rejects_oversized_request_headers() {
    let mut wire = format!("GET / HTTP/1.1\r\nX-Fill: {}\r\n", "a".repeat(65 * 1024)).into_bytes();
    let mut reader = wire.as_slice();

    let error = read_http1_request(&mut reader).expect_err("oversized headers should fail");

    assert_eq!(error, "VM HTTP request exceeded 64 KiB header limit");
    wire.clear();
}

#[test]
fn vm_http_rejects_early_request_body_eof() {
    let mut wire = "POST / HTTP/1.1\r\nContent-Length: 8\r\n\r\nshort".as_bytes();

    let error = read_http1_request(&mut wire).expect_err("early body EOF should fail");

    assert_eq!(error, "VM HTTP request body ended early");
}

#[test]
fn vm_http_incomplete_request_classifier_falls_back_after_complete_body_bytes() {
    let error =
        incomplete_http1_request_error(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nshort");

    assert_eq!(error, "VM HTTP request closed before headers completed");
}

#[test]
fn vm_http_incomplete_request_classifier_falls_back_after_malformed_headers() {
    let error = incomplete_http1_request_error(b"GET / HTTP/1.1\r\nbroken-header\r\n\r\n");

    assert_eq!(error, "VM HTTP request closed before headers completed");
}

#[test]
fn vm_http_rejects_non_utf8_request_body() {
    let bytes = b"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\n\xff";
    let mut wire = bytes.as_slice();

    let error = read_http1_request(&mut wire).expect_err("non-UTF-8 body should fail");

    assert!(error.contains("VM HTTP request body must be UTF-8"));
}

#[test]
fn vm_http_rejects_malformed_request_headers() {
    let mut wire = b"GET / HTTP/1.1\r\nbad header\r\n\r\n".as_slice();

    let error = read_http1_request(&mut wire).expect_err("malformed headers should fail");

    assert!(error.contains("failed to parse VM HTTP request"));
}

#[test]
fn vm_http_request_header_parser_reports_partial_headers() {
    let error = parse_http1_request_headers(b"GET / HTTP/1.1\r\n")
        .expect_err("partial request parse should fail");

    assert_eq!(error, "VM HTTP parser reported partial headers");
}

#[test]
fn vm_http_reports_request_read_error() {
    let mut reader = FailingReader::new("request read failed");

    let error = read_http1_request(&mut reader).expect_err("read error should fail");

    assert!(error.contains("failed to read VM HTTP request"));
}

#[test]
fn vm_http_rejects_invalid_request_content_length() {
    let mut wire = "POST / HTTP/1.1\r\nContent-Length: nope\r\n\r\n".as_bytes();

    let error = read_http1_request(&mut wire).expect_err("invalid length should fail");

    assert!(error.contains("VM HTTP Content-Length `nope` is invalid"));
}

#[test]
fn vm_http_rejects_oversized_request_body_declaration() {
    let mut wire = "POST / HTTP/1.1\r\nContent-Length: 1048577\r\n\r\n".as_bytes();

    let error = read_http1_request(&mut wire).expect_err("oversized body should fail");

    assert_eq!(error, "VM HTTP request exceeded 1 MiB body limit");
}

#[test]
fn vm_http_writes_http1_response_with_connection_policy() {
    let response = http::Response::builder()
        .status(203)
        .header(http::header::CONTENT_TYPE, "text/plain")
        .body("ok".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    write_http1_response(&mut wire, &response, false).expect("response should write");

    let text = String::from_utf8(wire).expect("response should be UTF-8");
    assert!(text.starts_with("HTTP/1.1 203 "));
    assert!(text.contains("Content-Length: 2\r\n"));
    assert!(text.contains("Connection: keep-alive\r\n"));
    assert!(text.contains("content-type: text/plain\r\n"));
    assert!(text.ends_with("\r\n\r\nok"));
}

#[test]
fn vm_http_writes_close_connection_policy() {
    let response = http::Response::builder()
        .status(200)
        .body("ok".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    write_http1_response(&mut wire, &response, true).expect("response should write");

    let text = String::from_utf8(wire).expect("response should be UTF-8");
    assert!(text.contains("Connection: close\r\n"));
}

#[test]
fn vm_http_writes_explicit_connection_upgrade_policy() {
    let response = http::Response::builder()
        .status(101)
        .header(http::header::CONNECTION, "Upgrade")
        .header(http::header::UPGRADE, "websocket")
        .header("sec-websocket-accept", "accept-key")
        .body(String::new())
        .expect("response should build");
    let mut wire = Vec::new();

    write_http1_response(&mut wire, &response, false).expect("response should write");

    let text = String::from_utf8(wire).expect("response should be UTF-8");
    assert!(text.starts_with("HTTP/1.1 101 Switching Protocols\r\n"));
    assert!(text.contains("Content-Length: 0\r\n"));
    assert!(text.contains("Connection: Upgrade\r\n"));
    assert!(text.contains("upgrade: websocket\r\n"));
    assert!(text.contains("sec-websocket-accept: accept-key\r\n"));
    assert!(!text.contains("Connection: keep-alive\r\n"));
    assert!(text.ends_with("\r\n\r\n"));
}

#[test]
fn vm_http_writes_explicit_content_length_policy() {
    let response = http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_LENGTH, "9")
        .body("short".to_string())
        .expect("response should build");
    let mut wire = Vec::new();

    write_http1_response(&mut wire, &response, false).expect("response should write");

    let text = String::from_utf8(wire).expect("response should be UTF-8");
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("Content-Length: 9\r\n"));
    assert!(text.contains("Connection: keep-alive\r\n"));
    assert!(!text.contains("Content-Length: 5\r\n"));
    assert!(text.ends_with("\r\n\r\nshort"));
}

#[test]
fn vm_http_in_memory_exchange_runs_typed_handler_and_writes_response() {
    let request = b"POST /items?kind=test HTTP/1.1\r\nHost: vm.local\r\nX-Trace: abc\r\nContent-Length: 5\r\n\r\nhello";
    let mut reader = Cursor::new(request.as_slice());
    let mut writer = Vec::new();

    let exchange = handle_http1_in_memory_exchange(&mut reader, &mut writer, false, |request| {
        assert_eq!(request.method(), http::Method::POST);
        assert_eq!(request.uri().path(), "/items");
        assert_eq!(request.uri().query(), Some("kind=test"));
        assert_eq!(
            request
                .headers()
                .get("x-trace")
                .and_then(|value| value.to_str().ok()),
            Some("abc")
        );
        assert_eq!(request.body(), "hello");
        http::Response::builder()
            .status(201)
            .header("X-Handled-By", "terlan-vm")
            .body("created".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("in-memory exchange should complete");

    assert_eq!(exchange.request_method, "POST");
    assert_eq!(exchange.request_path, "/items");
    assert_eq!(exchange.response_status, 201);
    assert_eq!(exchange.response_bytes, writer.len());
    assert!(!exchange.close_connection);
    let response = String::from_utf8(writer).expect("response is UTF-8");
    assert!(response.starts_with("HTTP/1.1 201 Created\r\n"));
    assert!(response.contains("Connection: keep-alive\r\n"));
    assert!(response.contains("x-handled-by: terlan-vm\r\n"));
    assert!(response.ends_with("\r\n\r\ncreated"));
}

#[test]
fn vm_http_in_memory_exchange_renders_template_response_through_handler_dispatch() {
    let request = b"GET /users/42 HTTP/1.1\r\nHost: vm.local\r\nContent-Length: 0\r\n\r\n";
    let mut reader = Cursor::new(request.as_slice());
    let mut writer = Vec::new();

    let exchange = handle_http1_in_memory_exchange(&mut reader, &mut writer, false, |request| {
        assert_eq!(request.method(), http::Method::GET);
        assert_eq!(request.uri().path(), "/users/42");
        let template = VmHttpTemplateResponse::html(
            "UserCard",
            "templates/user_card.terl.html",
            "<article>Ada</article>",
        )?;
        assert_eq!(template.source_file, "templates/user_card.terl.html");
        render_http_template_response(template, http::StatusCode::OK)
    })
    .expect("template handler exchange should complete");

    assert_eq!(exchange.request_method, "GET");
    assert_eq!(exchange.request_path, "/users/42");
    assert_eq!(exchange.response_status, 200);
    assert_eq!(exchange.response_bytes, writer.len());
    let response = String::from_utf8(writer).expect("response is UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("content-type: text/html; charset=utf-8\r\n"));
    assert!(response.contains("x-terlan-template: UserCard\r\n"));
    assert!(response.ends_with("\r\n\r\n<article>Ada</article>"));

    let empty_name = VmHttpTemplateResponse::html(" ", "templates/user_card.terl.html", "")
        .expect_err("template name is required");
    assert_eq!(empty_name, "VM HTTP template response name cannot be empty");

    let empty_source =
        VmHttpTemplateResponse::html("UserCard", " ", "").expect_err("source file is required");
    assert_eq!(
        empty_source,
        "VM HTTP template response source file cannot be empty"
    );
}

#[test]
fn vm_http_in_memory_exchange_runs_request_driven_arithmetic_handler() {
    let request = b"GET /add?a=2&b=40 HTTP/1.1\r\nHost: vm.local\r\nContent-Length: 0\r\n\r\n";
    let mut reader = Cursor::new(request.as_slice());
    let mut writer = Vec::new();

    let exchange = handle_http1_in_memory_exchange(&mut reader, &mut writer, true, |request| {
        assert_eq!(request.method(), http::Method::GET);
        assert_eq!(request.uri().path(), "/add");
        arithmetic_response_from_query(request.uri().query().unwrap_or(""))
    })
    .expect("arithmetic handler exchange should complete");

    assert_eq!(exchange.request_method, "GET");
    assert_eq!(exchange.request_path, "/add");
    assert_eq!(exchange.response_status, 200);
    assert!(exchange.close_connection);
    let response = String::from_utf8(writer).expect("response is UTF-8");
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response.contains("Content-Length: 2\r\n"));
    assert!(response.ends_with("\r\n\r\n42"));
}

#[test]
fn vm_http_in_memory_exchange_reports_malformed_request_before_handler() {
    let mut reader = Cursor::new(b"GET /broken HTTP/1.1\r\nBad Header\r\n\r\n".as_slice());
    let mut writer = Vec::new();

    let error =
        handle_http1_in_memory_exchange::<String>(&mut reader, &mut writer, true, |_request| {
            panic!("malformed request must not call handler");
        })
    .expect_err("malformed request should fail before handler execution");

    assert!(error.contains("failed to parse VM HTTP request"));
    assert!(writer.is_empty());
}

#[test]
fn vm_http_in_memory_exchange_reports_request_driven_handler_error() {
    let request = b"GET /add?a=2&b=nope HTTP/1.1\r\nHost: vm.local\r\nContent-Length: 0\r\n\r\n";
    let mut reader = Cursor::new(request.as_slice());
    let mut writer = Vec::new();

    let exchange = handle_http1_in_memory_exchange(&mut reader, &mut writer, true, |request| {
        assert_eq!(request.uri().path(), "/add");
        arithmetic_response_from_query(request.uri().query().unwrap_or(""))
    })
    .expect("invalid arithmetic input should still produce a typed response");

    assert_eq!(exchange.response_status, 400);
    let response = String::from_utf8(writer).expect("response is UTF-8");
    assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert!(response.ends_with("\r\n\r\ninvalid query integer"));
}

#[test]
fn vm_http_roundtrips_request_and_response_over_vm_tcp_streams() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let request_wire =
        b"POST /vm?page=1 HTTP/1.1\r\nHost: http.local\r\nContent-Length: 5\r\n\r\nhello";

    tcp.send(client, request_wire.to_vec())
        .expect("send request");
    let server_bytes = tcp
        .receive(server, 4096)
        .expect("receive request")
        .expect("request bytes");
    let mut request_reader = server_bytes.as_slice();
    let request = read_http1_request(&mut request_reader).expect("parse request");
    let response = http::Response::builder()
        .status(203)
        .header("x-vm-stream", "tcp")
        .body(format!("{}:{}", request.uri().path(), request.body()))
        .expect("response should build");
    let mut response_wire = Vec::new();

    write_http1_response(&mut response_wire, &response, true).expect("write response");
    tcp.send(server, response_wire).expect("send response");
    let client_bytes = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut response_reader = client_bytes.as_slice();
    let parsed_response =
        read_http1_response(&mut response_reader, 203).expect("parse response over VM TCP");
    let response_text = String::from_utf8(parsed_response).expect("response is UTF-8");

    assert!(response_text.contains("x-vm-stream: tcp\r\n"));
    assert!(response_text.ends_with("\r\n\r\n/vm:hello"));
}

#[test]
fn vm_http_tcp_server_reports_plaintext_transport_mode() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-http.local").expect("listener");
    let server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");

    assert_eq!(
        server.transport_mode(&tls).expect("transport mode"),
        VmTlsTransportMode::Plaintext
    );
    let request_resources = server.request_resource_metrics();
    assert_eq!(request_resources.active_body_buffers, 0);
    assert_eq!(request_resources.completed_requests, 0);
    assert!(server.request_resource_leaks().is_empty());
}

#[test]
fn vm_http_tcp_server_reports_tls_transport_mode() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-http.local").expect("listener");
    let server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan())
        .expect("manual plan");

    assert_eq!(
        server.transport_mode(&tls).expect("transport mode"),
        VmTlsTransportMode::Tls
    );
}

#[test]
fn vm_http_tcp_server_reports_missing_transport_plan() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("missing-http.local").expect("listener");
    let server = VmHttpTcpServer::new(listener, source("handler"));
    let tls = VmTlsRuntime::new();

    assert_eq!(
        server
            .transport_mode(&tls)
            .expect_err("missing plan should fail"),
        "VM TLS listener handle has no installed transport plan"
    );
}
