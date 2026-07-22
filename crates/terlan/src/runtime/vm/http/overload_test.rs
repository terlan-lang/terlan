use super::{
    overload::{VmHttpEnqueueOutcome, VmHttpOverloadConfig, VmHttpOverloadPolicy},
    VmHttpQueue, VmHttpTcpServer,
};
use crate::runtime::vm::{
    http_router::VmHttpRouter,
    process::{VmExitReason, VmProcessSource, VmProcessTable},
    tcp::VmTcpRuntime,
};

fn server_with_policy(
    tcp: &mut VmTcpRuntime,
    address: &str,
    policy: VmHttpOverloadPolicy,
) -> VmHttpTcpServer {
    server_with_policy_bound(tcp, address, policy, 1)
}

fn server_with_policy_bound(
    tcp: &mut VmTcpRuntime,
    address: &str,
    policy: VmHttpOverloadPolicy,
    max_pending: usize,
) -> VmHttpTcpServer {
    let listener = tcp.listen(address).expect("listener should bind");
    let overload =
        VmHttpOverloadConfig::new(policy, max_pending).expect("overload should validate");
    let router = VmHttpRouter::new()
        .overload(overload)
        .expect("router overload should install");
    VmHttpTcpServer::from_router(
        listener,
        VmProcessSource::new("app.Http", "handle", 1),
        &router,
    )
}

#[test]
fn vm_http_queue_overload_policies_preserve_full_queue_and_work_ownership() {
    let queue = VmHttpQueue::new(1).expect("queue should be created");
    queue.enqueue(1).expect("queue should accept first item");

    assert_eq!(
        queue
            .enqueue_with_policy(2, VmHttpOverloadPolicy::Reject)
            .expect("reject policy should not fail"),
        VmHttpEnqueueOutcome::Rejected(2)
    );
    assert_eq!(
        queue
            .enqueue_with_policy(3, VmHttpOverloadPolicy::Spill)
            .expect("spill policy should not fail"),
        VmHttpEnqueueOutcome::Spilled(3)
    );
    assert_eq!(queue.dequeue().expect("original item should remain"), 1);
}

#[test]
fn vm_http_queue_overload_policies_enqueue_when_capacity_is_available() {
    for policy in [
        VmHttpOverloadPolicy::Queue,
        VmHttpOverloadPolicy::Reject,
        VmHttpOverloadPolicy::Spill,
    ] {
        let queue = VmHttpQueue::new(1).expect("queue should be created");

        assert_eq!(
            queue
                .enqueue_with_policy(7, policy)
                .expect("available queue should accept item"),
            VmHttpEnqueueOutcome::Enqueued
        );
        assert_eq!(queue.dequeue().expect("accepted item should be queued"), 7);
        assert_eq!(queue.metrics().expect("metrics should read").max_depth, 1);
    }
}

#[test]
fn vm_http_server_queue_policy_backpressures_at_the_listener_bound() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_policy(&mut tcp, "queue.local", VmHttpOverloadPolicy::Queue);
    let _first = tcp.connect("queue.local", "first").expect("first connect");
    let _second = tcp
        .connect("queue.local", "second")
        .expect("second connect");

    let report = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 2, 2, |_request| {
            panic!("idle connections must not invoke the handler")
        })
        .expect("queue policy poll should run");
    let info = server.inspect(&tcp).expect("server should inspect");

    assert_eq!(report.accepted, 1);
    assert_eq!(report.rejected, 0);
    assert_eq!(report.spilled, 0);
    assert_eq!(report.parked, 1);
    assert_eq!(info.active_handlers, 1);
    assert_eq!(info.listener.queued_accepts, 1);
    assert_eq!(info.overload.map(|config| config.max_pending), Some(1));
}

#[test]
fn vm_http_server_reject_policy_closes_saturated_work_without_leaking_a_process() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_policy(&mut tcp, "reject.local", VmHttpOverloadPolicy::Reject);
    let _first = tcp.connect("reject.local", "first").expect("first connect");
    let second = tcp
        .connect("reject.local", "second")
        .expect("second connect");

    let report = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 2, 2, |_request| {
            panic!("idle connections must not invoke the handler")
        })
        .expect("reject policy poll should run");
    let info = server.inspect(&tcp).expect("server should inspect");

    assert_eq!(report.accepted, 2);
    assert_eq!(report.rejected, 1);
    assert_eq!(report.spilled, 0);
    assert_eq!(info.active_handlers, 1);
    assert_eq!(info.rejected_total, 1);
    assert_eq!(processes.live_process_ids().len(), 1);
    assert_eq!(
        tcp.send(second, b"late".to_vec())
            .expect_err("rejected peer must be closed"),
        "VM TCP peer stream is closed"
    );
}

#[test]
fn vm_http_server_spill_policy_reports_fallback_admission() {
    let mut processes = VmProcessTable::default();
    let mut tcp = VmTcpRuntime::new();
    let mut server = server_with_policy(&mut tcp, "spill.local", VmHttpOverloadPolicy::Spill);
    let _first = tcp.connect("spill.local", "first").expect("first connect");
    let _second = tcp
        .connect("spill.local", "second")
        .expect("second connect");

    let report = server
        .poll_keep_alive_with_limits(&mut processes, &mut tcp, 2, 2, |_request| {
            panic!("idle connections must not invoke the handler")
        })
        .expect("spill policy poll should run");
    let info = server.inspect(&tcp).expect("server should inspect");

    assert_eq!(report.accepted, 2);
    assert_eq!(report.rejected, 0);
    assert_eq!(report.spilled, 1);
    assert_eq!(report.parked, 2);
    assert_eq!(info.active_handlers, 2);
    assert_eq!(info.spilled_total, 1);
    assert_eq!(processes.live_process_ids().len(), 2);
}

#[test]
fn vm_http_server_saturation_stress_preserves_policy_accounting_and_cleanup() {
    const CONNECTIONS: usize = 64;
    const MAX_PENDING: usize = 8;
    let scenarios = [
        (VmHttpOverloadPolicy::Queue, 8, 0, 0, 56),
        (VmHttpOverloadPolicy::Reject, 64, 56, 0, 0),
        (VmHttpOverloadPolicy::Spill, 64, 0, 56, 0),
    ];

    for (index, (policy, accepted, rejected, spilled, queued)) in scenarios.into_iter().enumerate()
    {
        let address = format!("stress-{index}.local");
        let mut processes = VmProcessTable::default();
        let mut tcp = VmTcpRuntime::new();
        let mut server = server_with_policy_bound(&mut tcp, &address, policy, MAX_PENDING);
        let clients = (0..CONNECTIONS)
            .map(|client| {
                tcp.connect(&address, format!("client-{client}"))
                    .expect("stress client should connect")
            })
            .collect::<Vec<_>>();

        let report = server
            .poll_keep_alive_with_limits(
                &mut processes,
                &mut tcp,
                CONNECTIONS,
                CONNECTIONS,
                |_request| panic!("idle stress connections must not invoke the handler"),
            )
            .expect("saturation poll should run");
        let info = server.inspect(&tcp).expect("server should inspect");
        let active = accepted - rejected;

        assert_eq!(report.accepted, accepted, "policy {policy:?}");
        assert_eq!(report.rejected, rejected, "policy {policy:?}");
        assert_eq!(report.spilled, spilled, "policy {policy:?}");
        assert_eq!(report.polled, active, "policy {policy:?}");
        assert_eq!(report.parked, active, "policy {policy:?}");
        assert_eq!(info.active_handlers, active, "policy {policy:?}");
        assert_eq!(info.listener.queued_accepts, queued, "policy {policy:?}");
        assert_eq!(info.accepted_total, accepted, "policy {policy:?}");
        assert_eq!(info.rejected_total, rejected, "policy {policy:?}");
        assert_eq!(info.spilled_total, spilled, "policy {policy:?}");
        assert_eq!(processes.live_process_ids().len(), active);

        server
            .shutdown(&mut processes, &mut tcp, VmExitReason::Killed)
            .expect("stress server should shut down");
        assert!(processes.live_process_ids().is_empty());
        assert_eq!(server.active_handlers(), 0);
        assert_eq!(tcp.metrics().queued_accepts, 0);
        for client in clients {
            tcp.close_stream(client)
                .expect("stress client should close after server shutdown");
        }
        assert_eq!(tcp.metrics().open_streams, 0);
    }
}
