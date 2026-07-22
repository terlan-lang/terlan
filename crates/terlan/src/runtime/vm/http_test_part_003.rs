
#[test]
fn vm_http_tcp_exchange_runs_handler_and_sends_response_over_stream() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let request_wire =
        b"PUT /items/42 HTTP/1.1\r\nHost: http.local\r\nContent-Length: 7\r\n\r\npayload";

    tcp.send(client, request_wire.to_vec())
        .expect("send request");
    let exchange = handle_http1_tcp_exchange(&mut tcp, server, |request| {
        assert_eq!(request.method().as_str(), "PUT");
        assert_eq!(request.uri().path(), "/items/42");
        assert_eq!(request.body(), "payload");
        http::Response::builder()
            .status(202)
            .header("x-handler", "vm")
            .body("accepted".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("exchange should complete");

    assert_eq!(exchange.request_method, "PUT");
    assert_eq!(exchange.request_path, "/items/42");
    assert_eq!(exchange.response_status, 202);
    assert!(exchange.response_bytes >= "accepted".len());

    let client_bytes = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut response_reader = client_bytes.as_slice();
    let parsed_response =
        read_http1_response(&mut response_reader, 202).expect("parse response over VM TCP");
    let response_text = String::from_utf8(parsed_response).expect("response is UTF-8");

    assert!(response_text.contains("x-handler: vm\r\n"));
    assert!(response_text.ends_with("\r\n\r\naccepted"));
}

#[test]
fn vm_http_tcp_poll_exchange_waits_for_fragmented_request_then_completes() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    tcp.send(
        client,
        b"POST /poll HTTP/1.1\r\nHost: http.local\r\nContent-Length: 7\r\n\r\npay".to_vec(),
    )
    .expect("send partial request");
    let first = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |_request| {
        panic!("handler must not run before the request body is complete")
    })
    .expect("first poll should not fail");

    assert_eq!(first, VmHttpTcpPoll::NeedRead);
    assert_eq!(tcp.receive(client, 4096).expect("no response yet"), None);

    tcp.send(client, b"load".to_vec())
        .expect("send remaining body");
    let second = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |request| {
        assert_eq!(request.uri().path(), "/poll");
        assert_eq!(request.body(), "payload");
        http::Response::builder()
            .status(204)
            .body(String::new())
            .map_err(|error| error.to_string())
    })
    .expect("second poll should complete");

    match second {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "POST");
            assert_eq!(exchange.request_path, "/poll");
            assert_eq!(exchange.response_status, 204);
            assert!(exchange.response_bytes > 0);
        }
        VmHttpTcpPoll::NeedRead => panic!("second poll should complete"),
    }

    let client_bytes = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut response_reader = client_bytes.as_slice();
    let parsed_response =
        read_http1_response(&mut response_reader, 204).expect("parse polled response");
    let response_text = String::from_utf8(parsed_response).expect("response is UTF-8");

    assert!(response_text.starts_with("HTTP/1.1 204 "));
    assert!(response_text.ends_with("\r\n\r\n"));
}

#[test]
fn vm_http_request_body_stream_dispatches_ordered_bounded_chunks() {
    let mut wire =
        b"POST /stream HTTP/1.1\r\nHost: http.local\r\nContent-Length: 12\r\n\r\npayload-body"
            .as_slice();
    let request = read_http1_request(&mut wire).expect("request");

    let mut stream =
        stream_http_request_body_for_dispatch(&request, 4).expect("body dispatch stream");

    assert_eq!(stream.total_bytes(), 12);
    assert_eq!(stream.max_chunk_bytes(), 4);
    assert_eq!(stream.chunk_count(), 3);

    let first = stream.next_chunk().expect("first chunk");
    assert_eq!(first.index, 0);
    assert_eq!(first.bytes, b"payl");
    assert!(!first.is_final);

    let second = stream.next_chunk().expect("second chunk");
    assert_eq!(second.index, 1);
    assert_eq!(second.bytes, b"oad-");
    assert!(!second.is_final);

    let third = stream.next_chunk().expect("third chunk");
    assert_eq!(third.index, 2);
    assert_eq!(third.bytes, b"body");
    assert!(third.is_final);
    assert_eq!(stream.next_chunk(), None);

    let mut empty = http::Request::builder()
        .method("POST")
        .uri("/empty")
        .body(String::new())
        .expect("empty request");
    let mut empty_stream =
        stream_http_request_body_for_dispatch(&empty, 8).expect("empty body stream");
    let empty_chunk = empty_stream.next_chunk().expect("empty final chunk");
    assert_eq!(empty_chunk.bytes, Vec::<u8>::new());
    assert!(empty_chunk.is_final);

    *empty.body_mut() = "body".to_string();
    let error = stream_http_request_body_for_dispatch(&empty, 0).expect_err("zero chunk rejected");
    assert_eq!(
        error,
        "VM HTTP request body stream chunk size must be greater than zero"
    );
}

/// Verifies pollable VM HTTP rejects half-closed incomplete headers.
///
/// Inputs:
/// - A VM TCP client/server pair.
/// - Partial HTTP/1 header bytes followed by write-side EOF.
///
/// Output:
/// - Test passes when the poller returns a stable incomplete-header
///   diagnostic instead of parking forever.
///
/// Transformation:
/// - Locks VM TCP half-close integration at the HTTP parser boundary below
///   the serve adapter.
#[test]
fn vm_http_tcp_poll_exchange_rejects_half_closed_partial_headers() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    tcp.send(
        client,
        b"GET /poll HTTP/1.1\r\nHost: http.local\r\n".to_vec(),
    )
    .expect("send partial headers");
    tcp.close_write(client).expect("client request EOF");

    let error = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |_request| {
        panic!("handler must not run for partial headers")
    })
    .expect_err("partial headers should fail after EOF");

    assert_eq!(error, "VM HTTP request closed before headers completed");
    assert_eq!(tcp.receive(client, 4096).expect("no response yet"), None);
}

/// Verifies pollable VM HTTP rejects half-closed incomplete bodies.
///
/// Inputs:
/// - A VM TCP client/server pair.
/// - A complete HTTP/1 header declaring more body bytes than the client sends
///   before write-side EOF.
///
/// Output:
/// - Test passes when the poller reports early body EOF instead of treating
///   the stream as idle keep-alive.
///
/// Transformation:
/// - Keeps truncated request handling VM-owned and independently covered below
///   `terlc serve`.
#[test]
fn vm_http_tcp_poll_exchange_rejects_half_closed_truncated_body() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    tcp.send(
        client,
        b"POST /poll HTTP/1.1\r\nHost: http.local\r\nContent-Length: 8\r\n\r\nshort".to_vec(),
    )
    .expect("send truncated body");
    tcp.close_write(client).expect("client request EOF");

    let error = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |_request| {
        panic!("handler must not run for truncated body")
    })
    .expect_err("truncated body should fail after EOF");

    assert_eq!(error, "VM HTTP request body ended early");
    assert_eq!(tcp.receive(client, 4096).expect("no response yet"), None);
}

#[test]
fn vm_http_tcp_poll_exchange_rejects_buffered_truncated_body_after_late_eof() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    tcp.send(
        client,
        b"POST /poll HTTP/1.1\r\nHost: http.local\r\nContent-Length: 8\r\n\r\nshort".to_vec(),
    )
    .expect("send truncated body");

    let first = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |_request| {
        panic!("handler must not run before request EOF")
    })
    .expect("partial body should park for more bytes");
    assert_eq!(first, VmHttpTcpPoll::NeedRead);

    tcp.close_write(client).expect("client request EOF");
    let error = poll_http1_tcp_exchange(&mut tcp, server, &mut buffer, |_request| {
        panic!("handler must not run for buffered truncated body")
    })
    .expect_err("buffered truncated body should fail after EOF");

    assert_eq!(error, "VM HTTP request body ended early");
    assert_eq!(tcp.receive(client, 4096).expect("no response yet"), None);
}

#[test]
fn vm_http_tls_tcp_exchange_parses_decrypted_request_and_encrypts_response() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_exchange");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-tls.local").expect("listener");
    let client_stream = tcp
        .connect("http-tls.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let mut tls_stream = VmTlsTcpServerStream::new(server_stream, connection);
    let mut client = http_tls_client_for_cert(cert_der);
    let mut buffer = VmHttpTcpRequestBuffer::default();

    complete_http_tls_handshake(&mut client, &mut tcp, client_stream, &mut tls_stream);
    client
        .writer()
        .write_all(b"POST /secure HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 4\r\n\r\nbody")
        .expect("client writes HTTP request");
    flush_http_client_tls_to_tcp(&mut client, &mut tcp, client_stream);

    let poll = poll_http1_tls_tcp_exchange(&mut tcp, &mut tls_stream, &mut buffer, |request| {
        assert_eq!(request.method().as_str(), "POST");
        assert_eq!(request.uri().path(), "/secure");
        assert_eq!(request.body(), "body");
        http::Response::builder()
            .status(201)
            .body("created".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("HTTP over TLS poll");

    match poll {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "POST");
            assert_eq!(exchange.request_path, "/secure");
            assert_eq!(exchange.response_status, 201);
            assert!(exchange.close_connection);
        }
        VmHttpTcpPoll::NeedRead => panic!("HTTP over TLS request should complete"),
    }
    pump_http_tcp_to_tls_client(&mut tcp, client_stream, &mut client);
    let mut response = [0; 128];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads decrypted HTTP response");
    let response_text = std::str::from_utf8(&response[..read]).expect("response UTF-8");
    assert!(response_text.starts_with("HTTP/1.1 201 Created\r\n"));
    assert!(response_text.contains("Content-Length: 7\r\n"));
    assert!(response_text.ends_with("\r\n\r\ncreated"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_http_tls_tcp_exchange_completes_from_buffered_plaintext_before_transport_poll() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_buffered");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-tls-buffered.local").expect("listener");
    let client_stream = tcp
        .connect("http-tls-buffered.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let mut tls_stream = VmTlsTcpServerStream::new(server_stream, connection);
    let mut client = http_tls_client_for_cert(cert_der);
    let mut buffer = VmHttpTcpRequestBuffer::default();

    complete_http_tls_handshake(&mut client, &mut tcp, client_stream, &mut tls_stream);
    buffer
        .bytes
        .extend(b"GET /buffered HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 0\r\n\r\n");

    let poll = poll_http1_tls_tcp_exchange(&mut tcp, &mut tls_stream, &mut buffer, |request| {
        assert_eq!(request.method().as_str(), "GET");
        assert_eq!(request.uri().path(), "/buffered");
        http::Response::builder()
            .status(200)
            .body("buffered".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("buffered TLS HTTP poll");

    match poll {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "GET");
            assert_eq!(exchange.request_path, "/buffered");
            assert_eq!(exchange.response_status, 200);
        }
        VmHttpTcpPoll::NeedRead => panic!("buffered TLS request should complete"),
    }
    assert!(buffer.bytes.is_empty());

    pump_http_tcp_to_tls_client(&mut tcp, client_stream, &mut client);
    let mut response = [0; 128];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads decrypted buffered response");
    let response_text = std::str::from_utf8(&response[..read]).expect("response UTF-8");
    assert!(response_text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response_text.ends_with("\r\n\r\nbuffered"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_http_tls_tcp_exchange_waits_for_fragmented_encrypted_request_then_completes() {
    let (dir, cert_path, key_path, cert_der) =
        write_http_tls_cert_pair("http_tls_fragmented_exchange");
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-tls-fragmented.local").expect("listener");
    let client_stream = tcp
        .connect("http-tls-fragmented.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let mut tls_stream = VmTlsTcpServerStream::new(server_stream, connection);
    let mut client = http_tls_client_for_cert(cert_der);
    let mut buffer = VmHttpTcpRequestBuffer::default();

    complete_http_tls_handshake(&mut client, &mut tcp, client_stream, &mut tls_stream);
    client
        .writer()
        .write_all(
            b"POST /secure-fragment HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 7\r\n\r\npayload",
        )
        .expect("client writes HTTP request");
    let mut encrypted = Vec::new();
    client
        .write_tls(&mut encrypted)
        .expect("client writes encrypted request");
    assert!(encrypted.len() > 8);
    let split = encrypted.len() / 2;
    tcp.send(client_stream, encrypted[..split].to_vec())
        .expect("send encrypted request prefix");

    let mut handler_calls = 0;
    let first = poll_http1_tls_tcp_exchange(&mut tcp, &mut tls_stream, &mut buffer, |_request| {
        handler_calls += 1;
        http::Response::builder()
            .status(500)
            .body(String::new())
            .map_err(|error| error.to_string())
    })
    .expect("first TLS HTTP poll");

    assert_eq!(first, VmHttpTcpPoll::NeedRead);
    assert_eq!(handler_calls, 0);

    tcp.send(client_stream, encrypted[split..].to_vec())
        .expect("send encrypted request suffix");
    let second = poll_http1_tls_tcp_exchange(&mut tcp, &mut tls_stream, &mut buffer, |request| {
        handler_calls += 1;
        assert_eq!(request.method().as_str(), "POST");
        assert_eq!(request.uri().path(), "/secure-fragment");
        assert_eq!(request.body(), "payload");
        http::Response::builder()
            .status(202)
            .body("accepted".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("second TLS HTTP poll");

    match second {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "POST");
            assert_eq!(exchange.request_path, "/secure-fragment");
            assert_eq!(exchange.response_status, 202);
            assert!(exchange.response_bytes > 0);
        }
        VmHttpTcpPoll::NeedRead => panic!("second TLS HTTP poll should complete"),
    }
    assert_eq!(handler_calls, 1);

    pump_http_tcp_to_tls_client(&mut tcp, client_stream, &mut client);
    let mut response = [0; 128];
    let read = client
        .reader()
        .read(&mut response)
        .expect("client reads decrypted HTTP response");
    let response_text = std::str::from_utf8(&response[..read]).expect("response UTF-8");
    assert!(response_text.starts_with("HTTP/1.1 202 Accepted\r\n"));
    assert!(response_text.ends_with("\r\n\r\naccepted"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_http_tcp_keep_alive_exchange_preserves_pipelined_request_bytes() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();
    let pipelined = b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n\
GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n";

    tcp.send(client, pipelined.to_vec())
        .expect("send pipelined requests");
    let first = poll_http1_tcp_keep_alive_exchange(&mut tcp, server, &mut buffer, |request| {
        assert_eq!(request.uri().path(), "/one");
        http::Response::builder()
            .status(200)
            .body("first".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("first keep-alive exchange should complete");
    let second = poll_http1_tcp_keep_alive_exchange(&mut tcp, server, &mut buffer, |request| {
        assert_eq!(request.uri().path(), "/two");
        http::Response::builder()
            .status(201)
            .body("second".to_string())
            .map_err(|error| error.to_string())
    })
    .expect("second keep-alive exchange should complete from retained bytes");

    match first {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "GET");
            assert_eq!(exchange.request_path, "/one");
            assert_eq!(exchange.response_status, 200);
            assert!(!exchange.close_connection);
        }
        VmHttpTcpPoll::NeedRead => panic!("first pipelined request should complete"),
    }
    match second {
        VmHttpTcpPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "GET");
            assert_eq!(exchange.request_path, "/two");
            assert_eq!(exchange.response_status, 201);
            assert!(!exchange.close_connection);
        }
        VmHttpTcpPoll::NeedRead => panic!("second pipelined request should complete"),
    }

    let first_response = tcp
        .receive(client, 4096)
        .expect("receive first response")
        .expect("first response bytes");
    let second_response = tcp
        .receive(client, 4096)
        .expect("receive second response")
        .expect("second response bytes");
    let mut first_reader = first_response.as_slice();
    let mut second_reader = second_response.as_slice();
    let first_text = String::from_utf8(read_http1_response(&mut first_reader, 200).expect("first"))
        .expect("first response UTF-8");
    let second_text =
        String::from_utf8(read_http1_response(&mut second_reader, 201).expect("second"))
            .expect("second response UTF-8");

    assert!(first_text.contains("Connection: keep-alive\r\n"));
    assert!(first_text.ends_with("\r\n\r\nfirst"));
    assert!(second_text.contains("Connection: keep-alive\r\n"));
    assert!(second_text.ends_with("\r\n\r\nsecond"));
}

#[test]
fn vm_http_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter() {
    let mut processes = VmProcessTable::default();
    let handler = processes.spawn_root(source("http_handler"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    let first = poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler,
        server,
        &mut buffer,
        |_request| panic!("handler must not run before any request bytes arrive"),
    )
    .expect("empty stream should park");

    assert_eq!(first, VmHttpTcpActorPoll::Parked);
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Blocked
    );

    let (_sent, wakeups) = tcp
        .send_with_wakeups(
            client,
            b"POST /actor HTTP/1.1\r\nHost: http.local\r\nContent-Length: 4\r\n\r\ndone".to_vec(),
        )
        .expect("send complete request");
    let wake_report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(wake_report.read_wakeups, 1);
    assert!(wake_report.diagnostics.is_empty());
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("handler should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );

    let second = poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler,
        server,
        &mut buffer,
        |request| {
            assert_eq!(request.uri().path(), "/actor");
            assert_eq!(request.body(), "done");
            http::Response::builder()
                .status(200)
                .body("ok".to_string())
                .map_err(|error| error.to_string())
        },
    )
    .expect("complete request should respond");

    match second {
        VmHttpTcpActorPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "POST");
            assert_eq!(exchange.request_path, "/actor");
            assert_eq!(exchange.response_status, 200);
        }
        other => panic!("expected complete actor poll, got {other:?}"),
    }
    let response = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut reader = response.as_slice();

    assert!(
        String::from_utf8(read_http1_response(&mut reader, 200).expect("parse response"))
            .expect("response UTF-8")
            .ends_with("\r\n\r\nok")
    );
}

#[test]
fn vm_http_tls_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_actor");
    let mut processes = VmProcessTable::default();
    let handler = processes.spawn_root(source("tls_http_handler"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-tls-actor.local").expect("listener");
    let client_stream = tcp
        .connect("http-tls-actor.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let mut tls_stream = VmTlsTcpServerStream::new(server_stream, connection);
    let mut client = http_tls_client_for_cert(cert_der);
    let mut buffer = VmHttpTcpRequestBuffer::default();

    let first = poll_or_park_http1_tls_tcp_exchange_with_connection(
        &mut processes,
        &mut tcp,
        handler,
        &mut tls_stream,
        &mut buffer,
        true,
        None,
        |_request| panic!("handler must not run before TLS input arrives"),
    )
    .expect("empty TLS stream should park");

    assert_eq!(first, VmHttpTcpActorPoll::Parked);
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Blocked
    );

    let mut client_hello = Vec::new();
    client
        .write_tls(&mut client_hello)
        .expect("client writes TLS handshake bytes");
    let (_sent, wakeups) = tcp
        .send_with_wakeups(client_stream, client_hello)
        .expect("send client hello");
    let wake_report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(wake_report.read_wakeups, 1);
    assert!(wake_report.diagnostics.is_empty());
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("TLS handler should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );

    complete_http_tls_handshake(&mut client, &mut tcp, client_stream, &mut tls_stream);
    client
        .writer()
        .write_all(b"GET /actor-tls HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 0\r\n\r\n")
        .expect("client writes encrypted HTTP request");
    flush_http_client_tls_to_tcp(&mut client, &mut tcp, client_stream);

    let second = poll_or_park_http1_tls_tcp_exchange_with_connection(
        &mut processes,
        &mut tcp,
        handler,
        &mut tls_stream,
        &mut buffer,
        true,
        None,
        |request| {
            assert_eq!(request.uri().path(), "/actor-tls");
            http::Response::builder()
                .status(200)
                .body("tls-ok".to_string())
                .map_err(|error| error.to_string())
        },
    )
    .expect("complete TLS request should respond");

    match second {
        VmHttpTcpActorPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "GET");
            assert_eq!(exchange.request_path, "/actor-tls");
            assert_eq!(exchange.response_status, 200);
        }
        other => panic!("expected complete TLS actor poll, got {other:?}"),
    }
    let response_text = read_retained_http_tls_response(&mut tcp, client_stream, &mut client);
    assert!(response_text.ends_with("\r\n\r\ntls-ok"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_http_tls_tcp_actor_poll_rejects_missing_and_exited_handler_processes() {
    let (dir, cert_path, key_path, _cert_der) = write_http_tls_cert_pair("http_tls_actor_errors");
    let mut processes = VmProcessTable::default();
    let exited = processes.spawn_root(source("exited_tls_handler"));
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit handler");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http-tls-actor-errors.local").expect("listener");
    tcp.connect("http-tls-actor-errors.local", "tls_client")
        .expect("client stream");
    let server_stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let mut tls_stream = VmTlsTcpServerStream::new(server_stream, connection);
    let mut missing_buffer = VmHttpTcpRequestBuffer::default();
    let mut exited_buffer = VmHttpTcpRequestBuffer::default();

    assert_eq!(
        poll_or_park_http1_tls_tcp_exchange_with_connection(
            &mut processes,
            &mut tcp,
            missing,
            &mut tls_stream,
            &mut missing_buffer,
            true,
            None,
            |_request| panic!("missing process must fail before TLS handler")
        )
        .expect_err("missing TLS handler should fail"),
        "VM HTTP handler process 99 is missing"
    );
    assert_eq!(
        poll_or_park_http1_tls_tcp_exchange_with_connection(
            &mut processes,
            &mut tcp,
            exited,
            &mut tls_stream,
            &mut exited_buffer,
            true,
            None,
            |_request| panic!("exited process must fail before TLS handler")
        )
        .expect_err("exited TLS handler should fail"),
        "VM HTTP handler process 1 has exited"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn vm_http_tcp_actor_poll_rejects_missing_and_exited_handler_processes() {
    let mut processes = VmProcessTable::default();
    let exited = processes.spawn_root(source("exited_handler"));
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit handler");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut missing_buffer = VmHttpTcpRequestBuffer::default();
    let mut exited_buffer = VmHttpTcpRequestBuffer::default();

    assert_eq!(
        poll_or_park_http1_tcp_exchange(
            &mut processes,
            &mut tcp,
            missing,
            server,
            &mut missing_buffer,
            |_request| panic!("missing process must fail before handler")
        )
        .expect_err("missing handler should fail"),
        "VM HTTP handler process 99 is missing"
    );
    assert_eq!(
        poll_or_park_http1_tcp_exchange(
            &mut processes,
            &mut tcp,
            exited,
            server,
            &mut exited_buffer,
            |_request| panic!("exited process must fail before handler")
        )
        .expect_err("exited handler should fail"),
        "VM HTTP handler process 1 has exited"
    );
}
