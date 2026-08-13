use super::*;

#[test]
pub(super) fn vm_http_tcp_server_poll_with_tls_allows_plaintext_transport() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-poll.local").expect("listener");
    let client = tcp.connect("plain-poll.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");
    tcp.send(
        client,
        b"GET /plain HTTP/1.1\r\nHost: plain-poll.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send request");

    let report = server
        .poll_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/plain");
            http::Response::builder()
                .status(200)
                .body("plain".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("plaintext poll should run");

    assert_eq!(report.accepted, 1);
    assert_eq!(report.completed, 1);
    assert_eq!(server.active_handlers(), 0);

    let response = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut reader = response.as_slice();
    let response_text =
        String::from_utf8(read_http1_response(&mut reader, 200).expect("parse response"))
            .expect("response is UTF-8");

    assert!(response_text.ends_with("\r\n\r\nplain"));
}

#[test]
pub(super) fn vm_http_tcp_server_poll_with_tls_handles_encrypted_transport() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_server_poll");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-poll.local").expect("listener");
    let client = tcp.connect("tls-poll.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_with_tls(&mut processes, &mut tcp, &tls, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS poll accepts and parks");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.parked, 1);
    assert_eq!(server.active_handlers(), 1);

    let handler_process = server.handlers[0].process;
    complete_http_tls_handshake(
        &mut tls_client,
        &mut tcp,
        client,
        server.handlers[0]
            .tls_stream
            .as_mut()
            .expect("handler TLS stream"),
    );
    tls_client
        .writer()
        .write_all(b"GET /tls HTTP/1.1\r\nHost: tls-poll.local\r\nContent-Length: 0\r\n\r\n")
        .expect("client writes encrypted request");
    flush_http_client_tls_to_tcp(&mut tls_client, &mut tcp, client);
    processes
        .get_mut(handler_process)
        .expect("handler process")
        .wake();

    let second = server
        .poll_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/tls");
            http::Response::builder()
                .status(200)
                .body("tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS HTTP poll should complete");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.completed, 1);
    assert_eq!(server.active_handlers(), 0);
    pump_http_tcp_to_tls_client(&mut tcp, client, &mut tls_client);
    let mut response = [0; 128];
    let read = tls_client
        .reader()
        .read(&mut response)
        .expect("read decrypted response");
    let response_text = std::str::from_utf8(&response[..read]).expect("response UTF-8");
    assert!(response_text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(response_text.ends_with("\r\n\r\ntls"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_poll_with_tls_normalizes_cursor_after_closing_last_handler() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_close_cursor");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-close-cursor.local").expect("listener");
    let first_client = tcp
        .connect("tls-close-cursor.local", "first")
        .expect("first connect");
    let second_client = tcp
        .connect("tls-close-cursor.local", "second")
        .expect("second connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut first_tls_client = http_tls_client_for_cert(cert_der.clone());
    let mut second_tls_client = http_tls_client_for_cert(cert_der);

    let accepted = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 2, 2, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("initial TLS poll accepts both handlers");

    assert_eq!(accepted.accepted, 2);
    assert_eq!(accepted.parked, 2);
    assert_eq!(server.active_handlers(), 2);

    let first_handler = server.handlers[0].process;
    complete_http_tls_handshake(
        &mut first_tls_client,
        &mut tcp,
        first_client,
        server.handlers[0]
            .tls_stream
            .as_mut()
            .expect("first handler TLS stream"),
    );
    processes
        .get_mut(first_handler)
        .expect("first handler process")
        .wake();

    let second_handler = server.handlers[1].process;
    complete_http_tls_handshake(
        &mut second_tls_client,
        &mut tcp,
        second_client,
        server.handlers[1]
            .tls_stream
            .as_mut()
            .expect("second handler TLS stream"),
    );
    processes
        .get_mut(second_handler)
        .expect("second handler process")
        .wake();

    second_tls_client
        .writer()
        .write_all(b"GET /last-handler HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 0\r\n\r\n")
        .expect("second client writes encrypted request");
    flush_http_client_tls_to_tcp(&mut second_tls_client, &mut tcp, second_client);
    server.next_handler_index = 1;

    let closed = server
        .poll_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/last-handler");
            http::Response::builder()
                .status(200)
                .body("last".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS poll should close the last handler");

    assert_eq!(closed.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.next_handler_index, 0);
    let response_text =
        read_retained_http_tls_response(&mut tcp, second_client, &mut second_tls_client);
    assert!(response_text.ends_with("\r\n\r\nlast"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_allows_plaintext_transport() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-keep-alive.local").expect("listener");
    let client = tcp
        .connect("plain-keep-alive.local", "client")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");
    tcp.send(
        client,
        b"GET /keep HTTP/1.1\r\nHost: plain-keep-alive.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send request");

    let report = server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/keep");
            http::Response::builder()
                .status(200)
                .body("keep".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("plaintext keep-alive poll should run");

    assert_eq!(report.accepted, 1);
    assert_eq!(report.completed, 1);
    assert_eq!(server.active_handlers(), 1);

    let response = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut reader = response.as_slice();
    let response_text =
        String::from_utf8(read_http1_response(&mut reader, 200).expect("parse response"))
            .expect("response is UTF-8");

    assert!(response_text.contains("Connection: keep-alive\r\n"));
    assert!(response_text.ends_with("\r\n\r\nkeep"));
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_handles_encrypted_transport() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_keep_alive");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-keep-alive.local").expect("listener");
    let client = tcp
        .connect("tls-keep-alive.local", "client")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS keep-alive poll");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.parked, 1);
    assert_eq!(server.active_handlers(), 1);
    complete_retained_http_tls_server_handshake(
        &mut processes,
        &mut tcp,
        client,
        &mut tls_client,
        &mut server,
    );
    tls_client
        .writer()
        .write_all(
            b"GET /keep-tls HTTP/1.1\r\nHost: tls-keep-alive.local\r\nContent-Length: 0\r\n\r\n",
        )
        .expect("client writes encrypted request");
    flush_http_client_tls_to_tcp(&mut tls_client, &mut tcp, client);

    let second = server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/keep-tls");
            http::Response::builder()
                .status(200)
                .body("keep-tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS keep-alive request should complete");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    let response_text = read_retained_http_tls_response(&mut tcp, client, &mut tls_client);
    assert!(response_text.contains("Connection: keep-alive\r\n"));
    assert!(response_text.ends_with("\r\n\r\nkeep-tls"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_honors_connection_close_request() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_close");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-close.local").expect("listener");
    let client = tcp.connect("tls-close.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS keep-alive poll");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.parked, 1);
    complete_retained_http_tls_server_handshake(
        &mut processes,
        &mut tcp,
        client,
        &mut tls_client,
        &mut server,
    );
    tls_client
        .writer()
        .write_all(
            b"GET /close-tls HTTP/1.1\r\nHost: tls-close.local\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        )
        .expect("client writes close request");
    flush_http_client_tls_to_tcp(&mut tls_client, &mut tcp, client);

    let second = server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |request| {
            assert_eq!(request.uri().path(), "/close-tls");
            http::Response::builder()
                .status(200)
                .body("closing-tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS keep-alive close request should complete");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.completed, 1);
    assert_eq!(server.completed_total(), 1);
    assert_eq!(server.active_handlers(), 0);
    let response_text = read_retained_http_tls_response(&mut tcp, client, &mut tls_client);
    assert!(response_text.contains("Connection: close\r\n"));
    assert!(response_text.ends_with("\r\n\r\nclosing-tls"));
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("closed TLS peer should reject"),
        "VM TCP peer stream is closed"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_limits_preserves_scheduler_budgets() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-limited.local").expect("listener");
    let first_client = tcp
        .connect("plain-limited.local", "first")
        .expect("connect");
    let second_client = tcp
        .connect("plain-limited.local", "second")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");
    tcp.send(
        first_client,
        b"GET /first HTTP/1.1\r\nHost: plain-limited.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send first request");
    tcp.send(
        second_client,
        b"GET /second HTTP/1.1\r\nHost: plain-limited.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send second request");

    let report = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |request| {
            assert_eq!(request.uri().path(), "/first");
            http::Response::builder()
                .status(200)
                .body("first".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("limited plaintext keep-alive poll should run");

    assert_eq!(report.accepted, 1);
    assert_eq!(report.polled, 1);
    assert_eq!(report.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    assert_eq!(server.completed_total(), 1);
    assert!(tcp
        .receive(second_client, 4096)
        .expect("second pending")
        .is_none());
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_rejects_zero_limits_and_reports_idle() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-zero-limit.local").expect("listener");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");

    assert_eq!(
        server
            .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 0, 1, |_request| {
                panic!("zero accept limit should fail before handler execution")
            })
            .expect_err("zero accept limit should fail"),
        "VM HTTP server accept limit must be greater than 0"
    );
    assert_eq!(
        server
            .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 0, |_request| {
                panic!("zero handler limit should fail before handler execution")
            })
            .expect_err("zero handler limit should fail"),
        "VM HTTP server handler poll limit must be greater than 0"
    );

    let idle = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 2, 2, |_request| {
            panic!("idle listener should not invoke handler")
        })
        .expect("idle TLS HTTP poll should succeed");

    assert_eq!(idle, super::super::VmHttpTcpServerPoll::default());
    assert_eq!(server.active_handlers(), 0);
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_rejects_zero_limits_and_reports_encrypted_idle(
) {
    let (dir, cert_path, key_path, _cert_der) = write_http_tls_cert_pair("http_tls_zero_limit");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-zero-limit.local").expect("listener");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");

    assert_eq!(
        server
            .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 0, 1, |_request| {
                panic!("zero accept limit should fail before TLS handler execution")
            })
            .expect_err("zero TLS accept limit should fail"),
        "VM HTTP server accept limit must be greater than 0"
    );
    assert_eq!(
        server
            .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 0, |_request| {
                panic!("zero handler limit should fail before TLS handler execution")
            })
            .expect_err("zero TLS handler limit should fail"),
        "VM HTTP server handler poll limit must be greater than 0"
    );

    let idle = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 2, 2, |_request| {
            panic!("idle TLS listener should not invoke handler")
        })
        .expect("idle encrypted TLS HTTP poll should succeed");

    assert_eq!(idle, super::super::VmHttpTcpServerPoll::default());
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(server.accepted_total(), 0);
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_rejects_handler_without_tls_state() {
    let (dir, cert_path, key_path, _cert_der) =
        write_http_tls_cert_pair("http_missing_tls_handler_state");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-corrupt-tls.local").expect("listener");
    tcp.connect("plain-corrupt-tls.local", "client")
        .expect("connect");
    let handler = accept_http1_tcp_handler(&mut processes, &mut tcp, listener, source("handler"))
        .expect("accept plaintext handler")
        .expect("handler state");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    server.handlers.push(handler);

    let error = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("handler without TLS stream should fail before handler execution")
        })
        .expect_err("missing TLS stream state should fail");

    assert_eq!(error, "VM HTTP TLS handler missing TLS stream state");
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_accept_limit_preserves_accept_budget() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("plain-accept-budget.local").expect("listener");
    let first_client = tcp
        .connect("plain-accept-budget.local", "first")
        .expect("connect");
    let second_client = tcp
        .connect("plain-accept-budget.local", "second")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, plain_tls_plan())
        .expect("plain plan");
    tcp.send(
        first_client,
        b"GET /first HTTP/1.1\r\nHost: plain-accept-budget.local\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
    )
    .expect("send first request");
    tcp.send(
        second_client,
        b"GET /second HTTP/1.1\r\nHost: plain-accept-budget.local\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
    )
    .expect("send second request");

    let report = server
        .poll_keep_alive_with_tls_accept_limit(&mut processes, &mut tcp, &tls, 1, |request| {
            assert_eq!(request.uri().path(), "/first");
            http::Response::builder()
                .status(200)
                .body("first".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("accept-budgeted plaintext keep-alive poll should run");

    assert_eq!(report.accepted, 1);
    assert_eq!(report.completed, 1);
    assert_eq!(server.accepted_total(), 1);
    assert_eq!(server.active_handlers(), 1);
    assert!(tcp
        .receive(second_client, 4096)
        .expect("second pending")
        .is_none());
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_accept_limit_handles_encrypted_transport() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_accept_budget");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-accept-budget.local").expect("listener");
    let client = tcp
        .connect("tls-accept-budget.local", "client")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls_accept_limit(&mut processes, &mut tcp, &tls, 1, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS accept-budget poll");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.parked, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    complete_retained_http_tls_server_handshake(
        &mut processes,
        &mut tcp,
        client,
        &mut tls_client,
        &mut server,
    );
    tls_client
        .writer()
        .write_all(
            b"GET /accept-tls HTTP/1.1\r\nHost: tls-accept-budget.local\r\nContent-Length: 0\r\n\r\n",
        )
        .expect("client writes encrypted request");
    flush_http_client_tls_to_tcp(&mut tls_client, &mut tcp, client);

    let second = server
        .poll_keep_alive_with_tls_accept_limit(&mut processes, &mut tcp, &tls, 1, |request| {
            assert_eq!(request.uri().path(), "/accept-tls");
            http::Response::builder()
                .status(200)
                .body("accept-tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS accept-budget request should complete");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    let response_text = read_retained_http_tls_response(&mut tcp, client, &mut tls_client);
    assert!(response_text.contains("Connection: keep-alive\r\n"));
    assert!(response_text.ends_with("\r\n\r\naccept-tls"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_limits_handles_encrypted_transport() {
    let (dir, cert_path, key_path, cert_der) =
        write_http_tls_cert_pair("http_tls_scheduler_budget");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-limited.local").expect("listener");
    let client = tcp.connect("tls-limited.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS limited poll");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.polled, 1);
    assert_eq!(first.parked, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    complete_retained_http_tls_server_handshake(
        &mut processes,
        &mut tcp,
        client,
        &mut tls_client,
        &mut server,
    );
    tls_client
        .writer()
        .write_all(
            b"GET /limited-tls HTTP/1.1\r\nHost: tls-limited.local\r\nContent-Length: 0\r\n\r\n",
        )
        .expect("client writes encrypted request");
    flush_http_client_tls_to_tcp(&mut tls_client, &mut tcp, client);

    let second = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |request| {
            assert_eq!(request.uri().path(), "/limited-tls");
            http::Response::builder()
                .status(200)
                .body("limited-tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("TLS limited request should complete");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.polled, 1);
    assert_eq!(second.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    let response_text = read_retained_http_tls_response(&mut tcp, client, &mut tls_client);
    assert!(response_text.contains("Connection: keep-alive\r\n"));
    assert!(response_text.ends_with("\r\n\r\nlimited-tls"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_skips_blocked_encrypted_handler() {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_skip_blocked");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-skip-blocked.local").expect("listener");
    let _client = tcp
        .connect("tls-skip-blocked.local", "client")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let _tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS poll should accept and park");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.polled, 1);
    assert_eq!(first.parked, 1);
    assert_eq!(server.active_handlers(), 1);

    let second = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("blocked TLS handler must not be polled")
        })
        .expect("blocked TLS handler should be skipped");

    assert_eq!(second.accepted, 0);
    assert_eq!(second.polled, 0);
    assert_eq!(second.skipped_blocked, 1);
    assert_eq!(server.active_handlers(), 1);

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_rejects_missing_retained_handler_process() {
    let (dir, cert_path, key_path, _cert_der) =
        write_http_tls_cert_pair("http_tls_missing_handler_process");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-missing-handler.local").expect("listener");
    tcp.connect("tls-missing-handler.local", "client")
        .expect("connect");
    let stream = tcp
        .accept(listener, "tls_server")
        .expect("accept")
        .expect("server stream");
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let connection = tls
        .start_listener_server_connection(listener)
        .expect("server connection");
    let tls_stream = VmTlsTcpServerStream::new(stream, connection);
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    server.handlers.push(VmHttpTcpHandler {
        process: VmProcessId::from_raw_for_test(99),
        stream,
        buffer: VmHttpTcpRequestBuffer::default(),
        tls_stream: Some(tls_stream),
    });

    assert_eq!(
        server
            .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
                panic!("missing process must fail before TLS handler")
            })
            .expect_err("missing retained TLS handler process should fail"),
        "VM HTTP handler process 99 disappeared"
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
pub(super) fn vm_http_tcp_server_keep_alive_with_tls_reports_ready_when_more_encrypted_bytes_are_queued(
) {
    let (dir, cert_path, key_path, cert_der) = write_http_tls_cert_pair("http_tls_ready_queued");
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("tls-ready-queued.local").expect("listener");
    let client = tcp
        .connect("tls-ready-queued.local", "client")
        .expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handler"));
    let mut tls = VmTlsRuntime::new();
    tls.install_listener_plan(listener, manual_tls_plan_with_paths(cert_path, key_path))
        .expect("manual TLS plan");
    let mut tls_client = http_tls_client_for_cert(cert_der);

    let first = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("TLS handshake must not run HTTP handler")
        })
        .expect("first TLS poll should accept and park");

    assert_eq!(first.accepted, 1);
    assert_eq!(first.parked, 1);
    complete_retained_http_tls_server_handshake(
        &mut processes,
        &mut tcp,
        client,
        &mut tls_client,
        &mut server,
    );
    tls_client
        .writer()
        .write_all(b"GET /ready-tls HTTP/1.1\r\nHost: tls.local\r\nContent-Length: 0\r\n\r\n")
        .expect("client writes encrypted HTTP request");
    let mut encrypted = Vec::new();
    tls_client
        .write_tls(&mut encrypted)
        .expect("client writes encrypted request");
    assert!(encrypted.len() > 8);
    let split = encrypted.len() / 2;
    tcp.send(client, encrypted[..split].to_vec())
        .expect("send encrypted request prefix");
    tcp.send(client, encrypted[split..].to_vec())
        .expect("queue encrypted request suffix");

    let ready = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |_request| {
            panic!("first queued TLS poll should return ready before handler execution")
        })
        .expect("queued TLS poll should become ready");

    assert_eq!(ready.polled, 1);
    assert_eq!(ready.completed, 0);
    assert_eq!(ready.parked, 0);
    assert_eq!(
        processes
            .get(server.handlers[0].process)
            .expect("handler")
            .state,
        VmProcessState::Runnable
    );

    let completed = server
        .poll_keep_alive_with_tls_limits(&mut processes, &mut tcp, &tls, 1, 1, |request| {
            assert_eq!(request.uri().path(), "/ready-tls");
            http::Response::builder()
                .status(200)
                .body("ready-tls".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("second queued TLS poll should complete");

    assert_eq!(completed.polled, 1);
    assert_eq!(completed.completed, 1);
    let response_text = read_retained_http_tls_response(&mut tcp, client, &mut tls_client);
    assert!(response_text.ends_with("\r\n\r\nready-tls"));

    fs::remove_dir_all(dir).expect("cleanup");
}
