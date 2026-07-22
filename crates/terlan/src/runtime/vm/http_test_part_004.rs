
#[test]
fn vm_http_accepts_tcp_stream_into_handler_process_state() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");

    assert!(
        accept_http1_tcp_handler(&mut processes, &mut tcp, listener, source("handle"))
            .expect("empty accept")
            .is_none()
    );

    let client = tcp.connect("http.local", "client").expect("connect");
    let mut handler =
        accept_http1_tcp_handler(&mut processes, &mut tcp, listener, source("handle"))
            .expect("accept handler")
            .expect("handler state");

    assert_eq!(
        processes.get(handler.process).expect("handler").source,
        source("handle")
    );
    assert_eq!(
        tcp.inspect_stream(handler.stream)
            .expect("handler stream")
            .owner,
        Some("std.http.handler".to_string())
    );

    tcp.send(
        client,
        b"GET /accepted HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send request");
    let poll = poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler.process,
        handler.stream,
        &mut handler.buffer,
        |request| {
            assert_eq!(request.uri().path(), "/accepted");
            http::Response::builder()
                .status(200)
                .body("accepted".to_string())
                .map_err(|error| error.to_string())
        },
    )
    .expect("handler poll");

    match poll {
        VmHttpTcpActorPoll::Complete(exchange) => {
            assert_eq!(exchange.request_method, "GET");
            assert_eq!(exchange.request_path, "/accepted");
            assert_eq!(exchange.response_status, 200);
        }
        other => panic!("expected complete handler poll, got {other:?}"),
    }
    let response = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("response bytes");
    let mut reader = response.as_slice();
    let response_text = String::from_utf8(read_http1_response(&mut reader, 200).expect("parse"))
        .expect("response UTF-8");

    assert!(response_text.ends_with("\r\n\r\naccepted"));
}

#[test]
fn vm_http_finishes_tcp_handler_by_closing_stream_and_exiting_process() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut handler =
        accept_http1_tcp_handler(&mut processes, &mut tcp, listener, source("handle"))
            .expect("accept handler")
            .expect("handler state");
    processes
        .get_mut(handler.process)
        .expect("handler process")
        .add_resource_handle("http.request:1");

    tcp.send(
        client,
        b"GET /finish HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send request");
    let poll = poll_or_park_http1_tcp_exchange(
        &mut processes,
        &mut tcp,
        handler.process,
        handler.stream,
        &mut handler.buffer,
        |_request| {
            http::Response::builder()
                .status(200)
                .body("done".to_string())
                .map_err(|error| error.to_string())
        },
    )
    .expect("handler poll");

    assert!(matches!(poll, VmHttpTcpActorPoll::Complete(_)));
    let cleanup =
        finish_http1_tcp_handler(&mut processes, &mut tcp, &handler, VmExitReason::Normal)
            .expect("finish handler");

    assert_eq!(cleanup, vec!["http.request:1".to_string()]);
    assert_eq!(
        processes.get(handler.process).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Normal)
    );
    assert!(
        tcp.inspect_stream(handler.stream)
            .expect("handler stream")
            .closed
    );
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("closed peer should reject"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_finishes_cancelled_tcp_handler_with_error_reason() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    tcp.connect("http.local", "client").expect("connect");
    let handler = accept_http1_tcp_handler(&mut processes, &mut tcp, listener, source("handle"))
        .expect("accept handler")
        .expect("handler state");

    let cleanup = finish_http1_tcp_handler(
        &mut processes,
        &mut tcp,
        &handler,
        VmExitReason::Error("client disconnected".to_string()),
    )
    .expect("finish cancelled handler");

    assert!(cleanup.is_empty());
    assert_eq!(
        processes.get(handler.process).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Error("client disconnected".to_string()))
    );
    assert!(
        tcp.inspect_stream(handler.stream)
            .expect("handler stream")
            .closed
    );
}

#[test]
fn vm_http_tcp_server_polls_runnable_handlers_and_skips_parked_handlers() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        first_client,
        b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send first request");
    let first_poll = server
        .poll(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("first server poll");

    assert_eq!(first_poll.accepted, 2);
    assert_eq!(first_poll.polled, 2);
    assert_eq!(first_poll.completed, 1);
    assert_eq!(first_poll.parked, 1);
    assert_eq!(server.accepted_total(), 2);
    assert_eq!(server.completed_total(), 1);
    assert_eq!(server.active_handlers(), 1);

    let first_response = tcp
        .receive(first_client, 4096)
        .expect("first response receive")
        .expect("first response");
    let mut first_reader = first_response.as_slice();
    let first_response_text =
        String::from_utf8(read_http1_response(&mut first_reader, 200).expect("parse first"))
            .expect("first response UTF-8");
    assert!(first_response_text.ends_with("\r\n\r\nhandled:/one"));

    let idle_poll = server
        .poll(&mut processes, &mut tcp, |_request| {
            panic!("blocked handler must not be polled")
        })
        .expect("idle server poll");

    assert_eq!(idle_poll.accepted, 0);
    assert_eq!(idle_poll.polled, 0);
    assert_eq!(idle_poll.skipped_blocked, 1);

    let (_sent, wakeups) = tcp
        .send_with_wakeups(
            second_client,
            b"GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
        )
        .expect("send second request");
    let wake_report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);
    assert_eq!(wake_report.read_wakeups, 1);
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("second handler should run")
            .outcome,
        VmSchedulerOutcome::Ran
    );

    let second_poll = server
        .poll(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("second server poll");

    assert_eq!(second_poll.accepted, 0);
    assert_eq!(second_poll.polled, 1);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(server.completed_total(), 2);
    assert_eq!(server.active_handlers(), 0);

    let second_response = tcp
        .receive(second_client, 4096)
        .expect("second response receive")
        .expect("second response");
    let mut second_reader = second_response.as_slice();
    let second_response_text =
        String::from_utf8(read_http1_response(&mut second_reader, 200).expect("parse second"))
            .expect("second response UTF-8");
    assert!(second_response_text.ends_with("\r\n\r\nhandled:/two"));
}

#[test]
fn vm_http_tcp_server_keep_alive_reuses_handler_for_pipelined_requests() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));
    let pipelined = b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n\
GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n";

    tcp.send(client, pipelined.to_vec())
        .expect("send pipelined requests");
    let first_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("first keep-alive poll");

    assert_eq!(first_poll.accepted, 1);
    assert_eq!(first_poll.polled, 1);
    assert_eq!(first_poll.completed, 1);
    assert_eq!(server.completed_total(), 1);
    assert_eq!(server.active_handlers(), 1);

    let first_response = tcp
        .receive(client, 4096)
        .expect("first response receive")
        .expect("first response");
    let mut first_reader = first_response.as_slice();
    let first_response_text =
        String::from_utf8(read_http1_response(&mut first_reader, 200).expect("parse first"))
            .expect("first response UTF-8");

    assert!(first_response_text.contains("Connection: keep-alive\r\n"));
    assert!(first_response_text.ends_with("\r\n\r\nhandled:/one"));

    let second_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(201)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("second keep-alive poll");

    assert_eq!(second_poll.accepted, 0);
    assert_eq!(second_poll.polled, 1);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(server.completed_total(), 2);
    assert_eq!(server.active_handlers(), 1);

    let second_response = tcp
        .receive(client, 4096)
        .expect("second response receive")
        .expect("second response");
    let mut second_reader = second_response.as_slice();
    let second_response_text =
        String::from_utf8(read_http1_response(&mut second_reader, 201).expect("parse second"))
            .expect("second response UTF-8");

    assert!(second_response_text.contains("Connection: keep-alive\r\n"));
    assert!(second_response_text.ends_with("\r\n\r\nhandled:/two"));
}

#[test]
fn vm_http_tcp_server_keep_alive_parks_idle_handler_and_wakes_on_later_request() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        client,
        b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send first request");
    let first_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("first keep-alive poll");

    assert_eq!(first_poll.accepted, 1);
    assert_eq!(first_poll.completed, 1);
    assert_eq!(server.active_handlers(), 1);

    let first_response = tcp
        .receive(client, 4096)
        .expect("first response receive")
        .expect("first response");
    let mut first_reader = first_response.as_slice();
    let first_response_text =
        String::from_utf8(read_http1_response(&mut first_reader, 200).expect("parse first"))
            .expect("first response UTF-8");
    assert!(first_response_text.ends_with("\r\n\r\nhandled:/one"));

    let idle_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("idle keep-alive handler should park before handler execution")
        })
        .expect("idle keep-alive poll");

    assert_eq!(idle_poll.accepted, 0);
    assert_eq!(idle_poll.polled, 1);
    assert_eq!(idle_poll.parked, 1);
    assert_eq!(server.active_handlers(), 1);

    let (_sent, wakeups) = tcp
        .send_with_wakeups(
            client,
            b"GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
        )
        .expect("send second request");
    let wake_report = apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups);

    assert_eq!(wake_report.read_wakeups, 1);
    assert_eq!(
        scheduler
            .run_next(&mut processes, |_process, _slice| {
                VmSchedulerDecision::Yield { reductions: 1 }
            })
            .expect("keep-alive handler should run after read wake")
            .outcome,
        VmSchedulerOutcome::Ran
    );

    let second_poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            http::Response::builder()
                .status(201)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("second keep-alive poll");

    assert_eq!(second_poll.accepted, 0);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(server.completed_total(), 2);
    assert_eq!(server.active_handlers(), 1);

    let second_response = tcp
        .receive(client, 4096)
        .expect("second response receive")
        .expect("second response");
    let mut second_reader = second_response.as_slice();
    let second_response_text =
        String::from_utf8(read_http1_response(&mut second_reader, 201).expect("parse second"))
            .expect("second response UTF-8");
    assert!(second_response_text.ends_with("\r\n\r\nhandled:/two"));
}

#[test]
fn vm_http_tcp_server_keep_alive_accept_limit_bounds_accept_work_per_poll() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let _first_client = tcp.connect("http.local", "first").expect("first");
    let _second_client = tcp.connect("http.local", "second").expect("second");
    let _third_client = tcp.connect("http.local", "third").expect("third");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    assert_eq!(
        server
            .poll_keep_alive_with_accept_limit(&mut processes, &mut tcp, 0, |_request| {
                panic!("zero accept limit should fail before handler execution")
            })
            .expect_err("zero accept limit should fail"),
        "VM HTTP server accept limit must be greater than 0"
    );

    let first_poll = server
        .poll_keep_alive_with_accept_limit(&mut processes, &mut tcp, 2, |_request| {
            panic!("empty accepted streams should park before handler execution")
        })
        .expect("first limited poll");

    assert_eq!(first_poll.accepted, 2);
    assert_eq!(first_poll.parked, 2);
    assert_eq!(server.accepted_total(), 2);
    assert_eq!(server.active_handlers(), 2);

    let second_poll = server
        .poll_keep_alive_with_accept_limit(&mut processes, &mut tcp, 1, |_request| {
            panic!("empty accepted stream should park before handler execution")
        })
        .expect("second limited poll");

    assert_eq!(second_poll.accepted, 1);
    assert_eq!(second_poll.skipped_blocked, 2);
    assert_eq!(second_poll.parked, 1);
    assert_eq!(server.accepted_total(), 3);
    assert_eq!(server.active_handlers(), 3);
}

#[test]
fn vm_http_tcp_server_keep_alive_handler_limit_bounds_handler_work_per_poll() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let third_client = tcp.connect("http.local", "third").expect("third");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        first_client,
        b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send first request");
    tcp.send(
        second_client,
        b"GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send second request");
    tcp.send(
        third_client,
        b"GET /three HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send third request");

    assert_eq!(
        server
            .poll_keep_alive_with_limits(&mut processes, &mut tcp, 1, 0, |_request| {
                panic!("zero handler limit should fail before handler execution")
            })
            .expect_err("zero handler limit should fail"),
        "VM HTTP server handler poll limit must be greater than 0"
    );

    let first_poll = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 3, 2, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("first limited handler poll");

    assert_eq!(first_poll.accepted, 3);
    assert_eq!(first_poll.polled, 2);
    assert_eq!(first_poll.completed, 2);
    assert_eq!(server.accepted_total(), 3);
    assert_eq!(server.completed_total(), 2);
    assert_eq!(server.active_handlers(), 3);
    assert_eq!(
        tcp.receive(third_client, 4096).expect("third pending"),
        None
    );

    let first_response = tcp
        .receive(first_client, 4096)
        .expect("first response receive")
        .expect("first response");
    let second_response = tcp
        .receive(second_client, 4096)
        .expect("second response receive")
        .expect("second response");
    let mut first_reader = first_response.as_slice();
    let mut second_reader = second_response.as_slice();
    assert!(
        String::from_utf8(read_http1_response(&mut first_reader, 200).expect("parse first"))
            .expect("first response UTF-8")
            .ends_with("\r\n\r\nhandled:/one")
    );
    assert!(
        String::from_utf8(read_http1_response(&mut second_reader, 200).expect("parse second"))
            .expect("second response UTF-8")
            .ends_with("\r\n\r\nhandled:/two")
    );

    let second_poll = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 1, 3, |request| {
            http::Response::builder()
                .status(201)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("second limited handler poll");

    assert_eq!(second_poll.accepted, 0);
    assert_eq!(second_poll.polled, 3);
    assert_eq!(second_poll.parked, 2);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(server.completed_total(), 3);
    assert_eq!(server.active_handlers(), 3);

    let third_response = tcp
        .receive(third_client, 4096)
        .expect("third response receive")
        .expect("third response");
    let mut third_reader = third_response.as_slice();
    assert!(
        String::from_utf8(read_http1_response(&mut third_reader, 201).expect("parse third"))
            .expect("third response UTF-8")
            .ends_with("\r\n\r\nhandled:/three")
    );
}

#[test]
fn vm_http_tcp_server_keep_alive_handler_budget_uses_round_robin_cursor() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let third_client = tcp.connect("http.local", "third").expect("third");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        first_client,
        b"GET /one HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send first request");
    tcp.send(
        second_client,
        b"GET /two HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send second request");
    tcp.send(
        third_client,
        b"GET /three HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send third request");

    let first_poll = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 3, 1, |request| {
            http::Response::builder()
                .status(200)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("first round-robin poll");
    let second_poll = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 1, 1, |request| {
            http::Response::builder()
                .status(201)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("second round-robin poll");
    let third_poll = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 1, 1, |request| {
            http::Response::builder()
                .status(202)
                .body(format!("handled:{}", request.uri().path()))
                .map_err(|error| error.to_string())
        })
        .expect("third round-robin poll");

    assert_eq!(first_poll.polled, 1);
    assert_eq!(first_poll.completed, 1);
    assert_eq!(second_poll.polled, 1);
    assert_eq!(second_poll.completed, 1);
    assert_eq!(third_poll.polled, 1);
    assert_eq!(third_poll.completed, 1);
    assert_eq!(server.completed_total(), 3);

    let first_response = tcp
        .receive(first_client, 4096)
        .expect("first response receive")
        .expect("first response");
    let second_response = tcp
        .receive(second_client, 4096)
        .expect("second response receive")
        .expect("second response");
    let third_response = tcp
        .receive(third_client, 4096)
        .expect("third response receive")
        .expect("third response");
    let mut first_reader = first_response.as_slice();
    let mut second_reader = second_response.as_slice();
    let mut third_reader = third_response.as_slice();

    assert!(
        String::from_utf8(read_http1_response(&mut first_reader, 200).expect("parse first"))
            .expect("first response UTF-8")
            .ends_with("\r\n\r\nhandled:/one")
    );
    assert!(
        String::from_utf8(read_http1_response(&mut second_reader, 201).expect("parse second"))
            .expect("second response UTF-8")
            .ends_with("\r\n\r\nhandled:/two")
    );
    assert!(
        String::from_utf8(read_http1_response(&mut third_reader, 202).expect("parse third"))
            .expect("third response UTF-8")
            .ends_with("\r\n\r\nhandled:/three")
    );
}

#[test]
fn vm_http_tcp_server_keep_alive_honors_connection_close_request() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    tcp.send(
        client,
        b"GET /close HTTP/1.1\r\nHost: http.local\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
    )
    .expect("send close request");
    let poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |request| {
            assert_eq!(request.uri().path(), "/close");
            http::Response::builder()
                .status(200)
                .body("closing".to_string())
                .map_err(|error| error.to_string())
        })
        .expect("keep-alive poll should honor close");

    assert_eq!(poll.accepted, 1);
    assert_eq!(poll.polled, 1);
    assert_eq!(poll.completed, 1);
    assert_eq!(server.completed_total(), 1);
    assert_eq!(server.active_handlers(), 0);

    let response = tcp
        .receive(client, 4096)
        .expect("response receive")
        .expect("response");
    let mut reader = response.as_slice();
    let response_text =
        String::from_utf8(read_http1_response(&mut reader, 200).expect("parse response"))
            .expect("response UTF-8");

    assert!(response_text.contains("Connection: close\r\n"));
    assert!(response_text.ends_with("\r\n\r\nclosing"));
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("closed peer should reject"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_tcp_server_cancels_parked_handler_and_closes_stream() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));
    let first_poll = server
        .poll(&mut processes, &mut tcp, |_request| {
            panic!("empty stream should park before handler")
        })
        .expect("first poll");

    assert_eq!(first_poll.accepted, 1);
    assert_eq!(first_poll.parked, 1);
    assert_eq!(server.active_handlers(), 1);
    let handler_process = processes
        .get(VmProcessId::from_raw_for_test(1))
        .expect("handler process")
        .pid;
    processes
        .get_mut(handler_process)
        .expect("handler process")
        .add_resource_handle("http.stream:1");
    let cleanup = server
        .cancel_handler(
            &mut processes,
            &mut tcp,
            handler_process,
            VmExitReason::Error("client disconnected".to_string()),
        )
        .expect("cancel handler")
        .expect("handler was active");

    assert_eq!(cleanup, vec!["http.stream:1".to_string()]);
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(
        processes.get(handler_process).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Error("client disconnected".to_string()))
    );
    assert_eq!(
        server
            .cancel_handler(
                &mut processes,
                &mut tcp,
                handler_process,
                VmExitReason::Killed
            )
            .expect("second cancel is harmless"),
        None
    );
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("closed peer should reject"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_tcp_server_shutdown_closes_listener_and_active_handlers() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let first_client = tcp.connect("http.local", "first").expect("first");
    let second_client = tcp.connect("http.local", "second").expect("second");
    let mut server = VmHttpTcpServer::new(listener, source("handle"));

    let poll = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("empty handlers should park before request execution")
        })
        .expect("park handlers");

    assert_eq!(poll.accepted, 2);
    assert_eq!(poll.parked, 2);
    assert_eq!(server.active_handlers(), 2);

    let first_handler = VmProcessId::from_raw_for_test(1);
    let second_handler = VmProcessId::from_raw_for_test(2);
    processes
        .get_mut(first_handler)
        .expect("first handler")
        .add_resource_handle("http.stream:first");
    processes
        .get_mut(second_handler)
        .expect("second handler")
        .add_resource_handle("http.stream:second");
    let cleanup = server
        .shutdown(&mut processes, &mut tcp, VmExitReason::Killed)
        .expect("shutdown server");

    assert_eq!(
        cleanup,
        vec![
            "http.stream:first".to_string(),
            "http.stream:second".to_string()
        ]
    );
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(
        processes.get(first_handler).expect("first handler").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        processes.get(second_handler).expect("second handler").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        tcp.send(first_client, b"late".to_vec())
            .expect_err("first closed peer should reject"),
        "VM TCP peer stream is closed"
    );
    assert_eq!(
        tcp.send(second_client, b"late".to_vec())
            .expect_err("second closed peer should reject"),
        "VM TCP peer stream is closed"
    );
    assert_eq!(
        tcp.accept(listener, "late")
            .expect_err("closed listener should reject"),
        "VM TCP listener is closed"
    );
}
