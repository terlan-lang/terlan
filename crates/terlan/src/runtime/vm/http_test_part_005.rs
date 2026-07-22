
#[test]
fn vm_http_tcp_server_shutdown_with_tls_removes_listener_plan() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut tls = VmTlsRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("client");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));
    let plan = plain_tls_plan();

    tls.install_listener_plan(listener, plan.clone())
        .expect("listener TLS plan should install");
    let poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("empty handler should park before request execution")
        })
        .expect("park handler");
    assert_eq!(poll.accepted, 1);
    assert_eq!(poll.parked, 1);

    let handler = VmProcessId::from_raw_for_test(1);
    processes
        .get_mut(handler)
        .expect("handler")
        .add_resource_handle("http.stream:tls");
    let (cleanup, removed_plan) = server
        .shutdown_with_tls(&mut processes, &mut tcp, &mut tls, VmExitReason::Killed)
        .expect("shutdown with TLS");

    assert_eq!(cleanup, vec!["http.stream:tls".to_string()]);
    assert_eq!(removed_plan, Some(plan));
    assert_eq!(tls.inspect_listener_plan(listener), None);
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("closed peer should reject"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_tcp_server_noop_poll_and_empty_shutdown_are_stable() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    let poll = server
        .poll(&mut processes, &mut tcp, |_request| {
            panic!("no accepted streams means no handler execution")
        })
        .expect("empty poll should succeed");

    assert_eq!(poll, Default::default());
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(server.accepted_total(), 0);
    assert_eq!(server.completed_total(), 0);
    assert!(server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Normal)
        .expect("empty shutdown")
        .is_empty());
    assert_eq!(
        server
            .poll(&mut processes, &mut tcp, |_request| {
                panic!("closed listener should fail before handler execution")
            })
            .expect_err("polling closed listener should fail"),
        "VM TCP listener is closed"
    );
    assert_eq!(
        tcp.accept(listener, "late")
            .expect_err("closed listener should reject"),
        "VM TCP listener is closed"
    );
}

#[test]
fn vm_http_tcp_server_inspects_listener_pressure_and_handler_counters() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp
        .listen_with_backlog("http.local", 2)
        .expect("listen with backlog");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    let queued = server.inspect(&tcp).expect("inspect queued listener");
    assert_eq!(queued.listener.address, "http.local");
    assert_eq!(queued.listener.backlog_limit, 2);
    assert_eq!(queued.listener.queued_accepts, 2);
    assert_eq!(queued.listener.waiting_acceptors, 0);
    assert_eq!(queued.active_handlers, 0);
    assert_eq!(queued.accepted_total, 0);
    assert_eq!(queued.completed_total, 0);

    let poll = server
        .poll_keep_alive_with_accept_limit(&mut processes, &mut tcp, 2, |_request| {
            panic!("empty accepted streams should park before handler execution")
        })
        .expect("accept queued streams");
    assert_eq!(poll.accepted, 2);
    assert_eq!(poll.parked, 2);

    let parked = server.inspect(&tcp).expect("inspect parked handlers");
    assert_eq!(parked.listener.queued_accepts, 0);
    assert_eq!(parked.active_handlers, 2);
    assert_eq!(parked.next_handler_index, 0);
    assert_eq!(parked.accepted_total, 2);
    assert_eq!(parked.completed_total, 0);

    let (_sent, wakeups) = tcp
        .send_with_wakeups(
            first_client,
            b"GET /first HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
        )
        .expect("send first request");
    let wake_report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);
    assert_eq!(wake_report.read_wakeups, 1);
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("first handler should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );
    tcp.send(
        second_client,
        b"GET /second HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send second request");

    let completed = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 1, 1, |request| {
            ::http::Response::builder()
                .status(200)
                .body(request.uri().path().to_string())
                .map_err(|error| error.to_string())
        })
        .expect("complete one handler");
    assert_eq!(completed.completed, 1);

    let partial = server.inspect(&tcp).expect("inspect partial completion");
    assert_eq!(partial.active_handlers, 2);
    assert_eq!(partial.accepted_total, 2);
    assert_eq!(partial.completed_total, 1);

    server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Normal)
        .expect("shutdown");
    let closed = server.inspect(&tcp).expect("inspect closed listener");
    assert!(closed.listener.closed);
    assert_eq!(closed.active_handlers, 0);
}

#[test]
fn vm_http_tcp_server_propagates_handler_errors_without_finishing_handler() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        client,
        b"GET /fail HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send failing request");

    assert_eq!(
        server
            .poll(&mut processes, &mut tcp, |_request| {
                Err("handler failed".to_string())
            })
            .expect_err("handler failure should propagate"),
        "handler failed"
    );
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(
        processes
            .get(VmProcessId::from_raw_for_test(1))
            .expect("handler process")
            .state,
        VmProcessState::Runnable
    );
    assert_eq!(tcp.receive(client, 4096).expect("no response"), None);
}

/// Verifies keep-alive server polling propagates half-closed request EOF.
///
/// Inputs:
/// - One VM HTTP server with a client that sends an incomplete body and then
///   closes only the write side of the TCP stream.
///
/// Output:
/// - Test passes when the server returns a stable parser diagnostic, does not
///   park the handler forever, and does not write a response.
///
/// Transformation:
/// - Locks VM TCP half-close behavior at the retained HTTP-server layer, above
///   the raw poller and below the serve adapter.
#[test]
fn vm_http_tcp_server_keep_alive_reports_half_closed_truncated_body() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        client,
        b"POST /fail HTTP/1.1\r\nHost: http.local\r\nContent-Length: 8\r\n\r\nshort".to_vec(),
    )
    .expect("send truncated request");
    tcp.close_write(client).expect("client request EOF");

    assert_eq!(
        server
            .poll_keep_alive(&mut processes, &mut tcp, |_request| {
                panic!("handler must not run for truncated request")
            })
            .expect_err("truncated body should propagate"),
        "VM HTTP request body ended early"
    );
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    assert_eq!(server.completed_total(), 0);
    assert_eq!(
        processes
            .get(VmProcessId::from_raw_for_test(1))
            .expect("handler process")
            .state,
        VmProcessState::Runnable
    );
    assert_eq!(tcp.receive(client, 4096).expect("no response"), None);
}

/// Verifies keep-alive server polling propagates half-closed header EOF.
///
/// Inputs:
/// - One VM HTTP server with a client that sends incomplete HTTP/1 headers and
///   then closes only the write side of the TCP stream.
///
/// Output:
/// - Test passes when the server returns a stable incomplete-header diagnostic,
///   does not park the handler forever, and does not write a response.
///
/// Transformation:
/// - Completes retained-server EOF coverage for malformed requests that never
///   reach handler execution.
#[test]
fn vm_http_tcp_server_keep_alive_reports_half_closed_partial_headers() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        client,
        b"GET /fail HTTP/1.1\r\nHost: http.local\r\n".to_vec(),
    )
    .expect("send partial headers");
    tcp.close_write(client).expect("client request EOF");

    assert_eq!(
        server
            .poll_keep_alive(&mut processes, &mut tcp, |_request| {
                panic!("handler must not run for partial headers")
            })
            .expect_err("partial headers should propagate"),
        "VM HTTP request closed before headers completed"
    );
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.accepted_total(), 1);
    assert_eq!(server.completed_total(), 0);
    assert_eq!(
        processes
            .get(VmProcessId::from_raw_for_test(1))
            .expect("handler process")
            .state,
        VmProcessState::Runnable
    );
    assert_eq!(tcp.receive(client, 4096).expect("no response"), None);
}

#[test]
fn vm_http_tcp_server_reports_missing_retained_handler_process() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    tcp.connect("http.local", "client").expect("connect");
    let stream = tcp
        .accept(listener, "std.http.handler")
        .expect("accept")
        .expect("accepted stream");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));
    server.handlers.push(VmHttpTcpHandler {
        process: VmProcessId::from_raw_for_test(99),
        stream,
        buffer: VmHttpTcpRequestBuffer::default(),
        tls_stream: None,
    });

    assert_eq!(
        server
            .poll(&mut processes, &mut tcp, |_request| {
                panic!("missing process must fail before handler")
            })
            .expect_err("missing process should fail"),
        "VM HTTP handler process 99 disappeared"
    );
}

#[test]
fn vm_http_tcp_server_cancel_adjusts_round_robin_cursor_edges() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let third_client = tcp.connect("http.local", "third").expect("third");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    let poll = server
        .poll_keep_alive_with_accept_limit(&mut processes, &mut tcp, 3, |_request| {
            panic!("empty accepted streams should park before handler execution")
        })
        .expect("accept handlers");

    assert_eq!(poll.accepted, 3);
    assert_eq!(server.active_handlers(), 3);

    server.next_handler_index = 2;
    assert_eq!(
        server
            .cancel_handler(
                &mut processes,
                &mut tcp,
                VmProcessId::from_raw_for_test(1),
                VmExitReason::Killed,
            )
            .expect("cancel first")
            .expect("first handler cleanup"),
        Vec::<String>::new()
    );
    assert_eq!(server.next_handler_index, 1);
    assert_eq!(server.active_handlers(), 2);
    assert_eq!(
        tcp.send(first_client, b"late".to_vec())
            .expect_err("first stream closed"),
        "VM TCP peer stream is closed"
    );

    server.next_handler_index = 1;
    assert_eq!(
        server
            .cancel_handler(
                &mut processes,
                &mut tcp,
                VmProcessId::from_raw_for_test(3),
                VmExitReason::Killed,
            )
            .expect("cancel third")
            .expect("third handler cleanup"),
        Vec::<String>::new()
    );
    assert_eq!(server.next_handler_index, 0);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(
        tcp.send(third_client, b"late".to_vec())
            .expect_err("third stream closed"),
        "VM TCP peer stream is closed"
    );

    assert!(tcp.send(second_client, b"still-open".to_vec()).is_ok());
}

#[test]
fn vm_http_tcp_read_stream_handles_empty_buffer_and_eof() {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let _client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "std.http.handler")
        .expect("accept")
        .expect("accepted stream");
    let mut reader = VmTcpReadStream::new(&mut tcp, server);
    let mut empty = [];
    let mut buf = [0u8; 4];

    assert_eq!(
        io::Read::read(&mut reader, &mut empty).expect("empty read"),
        0
    );
    assert_eq!(io::Read::read(&mut reader, &mut buf).expect("eof read"), 0);
}

#[test]
fn vm_http_tcp_actor_poll_returns_ready_when_stream_has_more_data_before_parking() {
    let mut processes = VmProcessTable::default();
    let handler = processes.spawn_root(source("http_handler"));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let server = tcp
        .accept(listener, "http_handler")
        .expect("accept")
        .expect("accepted stream");
    let mut buffer = VmHttpTcpRequestBuffer::default();

    tcp.send(client, b"GET /ready HTTP/1.1\r\n".to_vec())
        .expect("send first incomplete chunk");
    tcp.send(
        client,
        b"Host: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send second queued chunk");

    let poll = poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler,
        server,
        &mut buffer,
        |_request| panic!("first poll has only incomplete headers"),
    )
    .expect("poll should return ready instead of parking");

    assert_eq!(poll, VmHttpTcpActorPoll::Ready);
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Runnable
    );
    match poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler,
        server,
        &mut buffer,
        |request| {
            assert_eq!(request.uri().path(), "/ready");
            http::Response::builder()
                .status(200)
                .body("ready".to_string())
                .map_err(|error| error.to_string())
        },
    )
    .expect("second poll should complete")
    {
        VmHttpTcpActorPoll::Complete(exchange) => {
            assert_eq!(exchange.request_path, "/ready");
            assert_eq!(exchange.response_status, 200);
        }
        other => panic!("expected ready request to complete, got {other:?}"),
    }
}

#[test]
fn vm_http_tcp_server_retains_ready_handler_without_parking() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(client, b"GET /ready HTTP/1.1\r\n".to_vec())
        .expect("send incomplete request start");
    tcp.send(
        client,
        b"Host: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send remaining request bytes");

    let first_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("first poll should see queued bytes and retry later")
        })
        .expect("first server poll");

    assert_eq!(first_poll.accepted, 1);
    assert_eq!(first_poll.polled, 1);
    assert_eq!(first_poll.parked, 0);
    assert_eq!(first_poll.completed, 0);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(
        processes
            .get(VmProcessId::from_raw_for_test(1))
            .expect("handler process")
            .state,
        VmProcessState::Runnable
    );

    let second_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            assert_eq!(request.uri().path(), "/ready");
            http::Response::builder()
                .status(200)
                .body("ready".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("second server poll");

    assert_eq!(second_poll.accepted, 0);
    assert_eq!(second_poll.polled, 1);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(server.active_handlers(), 1);
    assert_eq!(server.completed_total(), 1);

    let response = tcp
        .receive(client, 4096)
        .expect("response receive")
        .expect("response bytes");
    let mut reader = response.as_slice();
    let text = String::from_utf8(read_http1_response(&mut reader, 200).expect("parse response"))
        .expect("response UTF-8");

    assert!(text.ends_with("\r\n\r\nready"));
}

#[test]
fn vm_http_buffer_parser_reports_incomplete_limit_edges() {
    let oversized_header = vec![b'a'; 64 * 1024 + 1];
    let oversized_body = format!(
        "POST /big HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        1024 * 1024 + 1
    );

    assert_eq!(
        super::try_parse_http1_request_buffer(&oversized_header)
            .expect_err("oversized incomplete header should fail"),
        "VM HTTP request exceeded 64 KiB header limit"
    );
    assert_eq!(
        super::try_parse_http1_request_buffer(oversized_body.as_bytes())
            .expect_err("oversized declared body should fail"),
        "VM HTTP request exceeded 1 MiB body limit"
    );
}
