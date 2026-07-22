use super::VmHttpTcpServer;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::VmScheduler,
    tcp::VmTcpRuntime,
    timer::{VmTimerEvent, VmTimerKind, VmTimerTable},
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn vm_http_tcp_server_deadline_cancels_parked_handler_and_closes_stream() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut scheduler = VmScheduler::default();
    let mut timers = VmTimerTable::default();
    let mut server = VmHttpTcpServer::with_handler_timeout_ticks(listener, source("handle"), 5)
        .expect("deadline server");

    let accepted = server
        .poll_keep_alive_with_deadlines(
            &mut processes,
            &mut tcp,
            &mut timers,
            &mut scheduler,
            10,
            |_request| panic!("empty stream should park before handler"),
        )
        .expect("accept and park");
    let handler = VmProcessId::from_raw_for_test(1);

    assert_eq!(accepted.http.accepted, 1);
    assert_eq!(accepted.http.parked, 1);
    assert!(accepted.timer_events.is_empty());
    assert_eq!(timers.snapshots()[0].deadline_tick, 15);

    let expired = server
        .poll_keep_alive_with_deadlines(
            &mut processes,
            &mut tcp,
            &mut timers,
            &mut scheduler,
            15,
            |_request| panic!("expired handler must not execute"),
        )
        .expect("expire handler");

    assert_eq!(expired.timed_out_handlers, vec![handler]);
    assert!(matches!(
        expired.timer_events.as_slice(),
        [VmTimerEvent::Fired {
            owner,
            kind: VmTimerKind::OneShot,
            ..
        }] if *owner == handler
    ));
    assert_eq!(server.active_handlers(), 0);
    assert!(timers.snapshots().is_empty());
    assert_eq!(
        processes.get(handler).expect("handler").state,
        VmProcessState::Exited(VmExitReason::Error(
            "http_request_deadline_exceeded".to_string()
        ))
    );
    assert_eq!(
        tcp.send(client, b"late".to_vec())
            .expect_err("deadline closes stream"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_tcp_server_completion_cancels_request_deadline() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let client = tcp.connect("http.local", "client").expect("connect");
    let mut scheduler = VmScheduler::default();
    let mut timers = VmTimerTable::default();
    let mut server = VmHttpTcpServer::with_handler_timeout_ticks(listener, source("handle"), 5)
        .expect("deadline server");

    server
        .poll_keep_alive_with_deadlines(
            &mut processes,
            &mut tcp,
            &mut timers,
            &mut scheduler,
            10,
            |_request| panic!("empty stream should park before handler"),
        )
        .expect("accept and park");
    tcp.send(
        client,
        b"GET /ready HTTP/1.1\r\nHost: http.local\r\nContent-Length: 0\r\n\r\n".to_vec(),
    )
    .expect("send request");
    processes
        .get_mut(VmProcessId::from_raw_for_test(1))
        .expect("handler")
        .wake();

    let completed = server
        .poll_keep_alive_with_deadlines(
            &mut processes,
            &mut tcp,
            &mut timers,
            &mut scheduler,
            12,
            |_request| {
                http::Response::builder()
                    .status(200)
                    .body("ready".to_string())
                    .map_err(|error| error.to_string())
            },
        )
        .expect("complete before deadline");

    assert_eq!(completed.http.completed, 1);
    assert!(completed.timed_out_handlers.is_empty());
    assert!(matches!(
        completed.timer_events.as_slice(),
        [VmTimerEvent::Cancelled {
            owner,
            kind: VmTimerKind::OneShot,
            ..
        }] if *owner == VmProcessId::from_raw_for_test(1)
    ));
    assert!(timers.snapshots().is_empty());
    assert_eq!(server.active_handlers(), 1);
}

#[test]
fn vm_http_tcp_server_deadline_rejects_zero_and_overflow_without_accepting() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("http.local").expect("listen");
    let mut scheduler = VmScheduler::default();
    let mut timers = VmTimerTable::default();

    assert_eq!(
        VmHttpTcpServer::with_handler_timeout_ticks(listener, source("handle"), 0)
            .err()
            .expect("zero timeout must fail"),
        "VM HTTP handler timeout must be greater than 0 ticks"
    );
    let mut server = VmHttpTcpServer::with_handler_timeout_ticks(listener, source("handle"), 2)
        .expect("deadline server");
    assert_eq!(
        server
            .poll_keep_alive_with_deadlines(
                &mut processes,
                &mut tcp,
                &mut timers,
                &mut scheduler,
                u64::MAX,
                |_request| panic!("overflow must fail before polling"),
            )
            .expect_err("deadline overflow"),
        format!("VM HTTP handler deadline overflow at tick {}", u64::MAX)
    );
    assert_eq!(server.active_handlers(), 0);
    assert!(timers.snapshots().is_empty());
}
