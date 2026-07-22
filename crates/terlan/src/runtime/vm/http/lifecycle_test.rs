use super::{lifecycle::VmHttpDrainOutcome, VmHttpTcpServer};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::{VmScheduler, VmSchedulerConfig},
    tcp::VmTcpRuntime,
    tcp_scheduler::apply_tcp_wakeups,
    tls::{VmTlsMode, VmTlsPlan, VmTlsRuntime},
};

fn source() -> VmProcessSource {
    VmProcessSource::new("app.Main", "handle", 0)
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

#[test]
fn vm_http_tcp_server_drain_rejects_invalid_transitions_without_closing_listener() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("drain.local").expect("listen");
    let mut server = VmHttpTcpServer::new(listener, source());

    assert_eq!(
        server
            .poll_drain(
                &mut processes,
                &mut tcp,
                VmExitReason::Killed,
                |_request| panic!("drain has not started"),
            )
            .expect_err("poll before begin should fail"),
        "VM HTTP server drain has not started"
    );
    assert_eq!(
        server
            .begin_drain(&mut tcp, 0)
            .expect_err("zero drain budget should fail"),
        "VM HTTP drain poll limit must be greater than 0"
    );
    let _client = tcp
        .connect("drain.local", "still-open")
        .expect("failed drain start must leave listener open");

    server.begin_drain(&mut tcp, 1).expect("begin drain");
    assert_eq!(
        server
            .begin_drain(&mut tcp, 1)
            .expect_err("second begin should fail"),
        "VM HTTP server is already draining"
    );
    let report = server
        .poll_drain(&mut processes, &mut tcp, VmExitReason::Killed, |_request| {
            panic!("no handler was accepted before drain")
        })
        .expect("empty drain should finish");
    assert_eq!(report.outcome, VmHttpDrainOutcome::Drained);
    assert_eq!(report.polls, 0);
    assert_eq!(report.completed_handlers, 0);
    assert_eq!(report.forced_handlers, 0);
    assert!(report.cleanup.is_empty());
    assert_eq!(
        server
            .poll_drain(
                &mut processes,
                &mut tcp,
                VmExitReason::Killed,
                |_request| panic!("stopped drain should not poll"),
            )
            .expect_err("terminal drain should reject another poll"),
        "VM HTTP server is stopped"
    );
}

#[test]
fn vm_http_tcp_server_drain_completes_woken_handler_before_deadline() {
    let mut processes = VmProcessTable::default();
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 10));
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("drain.local").expect("listen");
    let client = tcp.connect("drain.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source());

    let accepted = server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("empty stream should park")
        })
        .expect("accept handler");
    assert_eq!(accepted.accepted, 1);
    assert_eq!(accepted.parked, 1);

    let (_sent, wakeups) = tcp
        .send_with_wakeups(
            client,
            b"GET /ready HTTP/1.1\r\nHost: drain.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
        )
        .expect("send request");
    assert_eq!(
        apply_tcp_wakeups(&mut processes, &mut scheduler, wakeups).read_wakeups,
        1
    );
    server.begin_drain(&mut tcp, 3).expect("begin drain");
    assert_eq!(
        tcp.accept(listener, "late")
            .expect_err("drain must stop accepts"),
        "VM TCP listener is closed"
    );

    let report = server
        .poll_drain(
            &mut processes,
            &mut tcp,
            VmExitReason::Error("drain deadline exceeded".to_string()),
            |request| {
                assert_eq!(request.uri().path(), "/ready");
                http::Response::builder()
                    .status(200)
                    .body("drained".to_string())
                    .map_err(|error| error.to_string())
            },
        )
        .expect("drain handler");

    assert_eq!(report.outcome, VmHttpDrainOutcome::Drained);
    assert_eq!(report.polls, 1);
    assert_eq!(report.completed_handlers, 1);
    assert_eq!(report.forced_handlers, 0);
    assert!(report.cleanup.is_empty());
    assert_eq!(server.active_handlers(), 0);
    assert_eq!(server.completed_total(), 1);
    let response = tcp
        .receive(client, 4096)
        .expect("receive response")
        .expect("drain response");
    assert!(String::from_utf8(response)
        .expect("response UTF-8")
        .ends_with("\r\n\r\ndrained"));
}

#[test]
fn vm_http_tcp_server_drain_forces_parked_handler_at_deadline() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("drain.local").expect("listen");
    let client = tcp.connect("drain.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source());

    server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("empty stream should park")
        })
        .expect("accept handler");
    let handler = VmProcessId::from_raw_for_test(1);
    processes
        .get_mut(handler)
        .expect("handler")
        .add_resource_handle("http.stream:drain");
    server.begin_drain(&mut tcp, 2).expect("begin drain");

    let pending = server
        .poll_drain(
            &mut processes,
            &mut tcp,
            VmExitReason::Error("drain deadline exceeded".to_string()),
            |_request| panic!("parked handler has no request"),
        )
        .expect("first drain tick");
    assert_eq!(pending.outcome, VmHttpDrainOutcome::Pending);
    assert_eq!(pending.polls, 1);
    assert_eq!(pending.forced_handlers, 0);

    let forced = server
        .poll_drain(
            &mut processes,
            &mut tcp,
            VmExitReason::Error("drain deadline exceeded".to_string()),
            |_request| panic!("parked handler has no request"),
        )
        .expect("deadline drain tick");
    assert_eq!(forced.outcome, VmHttpDrainOutcome::Forced);
    assert_eq!(forced.polls, 2);
    assert_eq!(forced.completed_handlers, 0);
    assert_eq!(forced.forced_handlers, 1);
    assert_eq!(forced.cleanup, vec!["http.stream:drain".to_string()]);
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Error("drain deadline exceeded".to_string()))
    );
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("forced drain closes peer"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_tcp_server_drain_does_not_count_cancellation_as_completion() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("drain.local").expect("listen");
    let _client = tcp.connect("drain.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source());

    server
        .poll_keep_alive(&mut processes, &mut tcp, |_request| {
            panic!("empty stream should park")
        })
        .expect("accept handler");
    let handler = VmProcessId::from_raw_for_test(1);
    server.begin_drain(&mut tcp, 2).expect("begin drain");
    server
        .cancel_handler(&mut processes, &mut tcp, handler, VmExitReason::Killed)
        .expect("cancel handler")
        .expect("handler should be active");

    let report = server
        .poll_drain(&mut processes, &mut tcp, VmExitReason::Killed, |_request| {
            panic!("canceled handler must not execute")
        })
        .expect("finish empty drain");
    assert_eq!(report.outcome, VmHttpDrainOutcome::Drained);
    assert_eq!(report.completed_handlers, 0);
    assert_eq!(report.forced_handlers, 0);
}

#[test]
fn vm_http_tcp_server_tls_drain_removes_plan_only_after_terminal_tick() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut tls = VmTlsRuntime::new();
    let listener = tcp.listen("drain.local").expect("listen");
    let _client = tcp.connect("drain.local", "client").expect("connect");
    let mut server = VmHttpTcpServer::new(listener, source());
    let plan = plain_tls_plan();
    tls.install_listener_plan(listener, plan.clone())
        .expect("install plan");
    server
        .poll_keep_alive_with_tls(&mut processes, &mut tcp, &tls, |_request| {
            panic!("empty stream should park")
        })
        .expect("accept handler");
    server.begin_drain(&mut tcp, 2).expect("begin drain");

    let (pending, removed) = server
        .poll_drain_with_tls(
            &mut processes,
            &mut tcp,
            &mut tls,
            VmExitReason::Killed,
            |_request| panic!("parked handler has no request"),
        )
        .expect("pending TLS drain");
    assert_eq!(pending.outcome, VmHttpDrainOutcome::Pending);
    assert_eq!(removed, None);
    assert_eq!(tls.inspect_listener_plan(listener), Some(&plan));

    let (forced, removed) = server
        .poll_drain_with_tls(
            &mut processes,
            &mut tcp,
            &mut tls,
            VmExitReason::Killed,
            |_request| panic!("parked handler has no request"),
        )
        .expect("terminal TLS drain");
    assert_eq!(forced.outcome, VmHttpDrainOutcome::Forced);
    assert_eq!(removed, Some(plan));
    assert_eq!(tls.inspect_listener_plan(listener), None);
}
