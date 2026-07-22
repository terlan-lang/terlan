use super::*;
use crate::runtime::vm::{
    actor::{VmActorReceive, VmActorRuntime, VmActorSpawnOptions},
    process::{VmExitReason, VmProcessId, VmProcessSource},
    ReplValue,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.PoolParity", name, 0)
}

#[test]
fn stdlib_pool_places_linked_worker_and_roundtrips_request() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .update_load("node-a", 3)
        .expect("node-a load should update");
    scheduler
        .update_load("node-b", 1)
        .expect("node-b load should update");

    let placement = scheduler
        .place("echo-worker", &VmPlacementPolicy::LeastConnections)
        .expect("least-loaded worker placement should succeed");
    assert_eq!(
        placement,
        decision("echo-worker", "node-b", "least_connections", false)
    );

    let mut runtime = VmActorRuntime::default();
    let caller = runtime.spawn_root(source("caller"));
    let worker = runtime
        .spawn_child_with_options(
            caller,
            source("echo"),
            VmActorSpawnOptions::default().linked(),
        )
        .expect("linked worker should spawn")
        .pid;
    let request = ReplValue::Tuple(vec![
        ReplValue::Atom("echo".to_string()),
        ReplValue::Int(41),
    ]);

    runtime
        .send(caller, worker, request.clone())
        .expect("request should reach worker");
    let VmActorReceive::Message(received) = runtime
        .receive_next_or_block(worker)
        .expect("worker should receive request")
    else {
        panic!("worker request must not block");
    };
    assert_eq!(received.payload, request);

    runtime
        .send(worker, caller, received.payload)
        .expect("reply should reach caller");
    let VmActorReceive::Message(reply) = runtime
        .receive_next_or_block(caller)
        .expect("caller should receive reply")
    else {
        panic!("caller reply must not block");
    };
    assert_eq!(reply.payload, request);

    runtime
        .exit_actor(worker, VmExitReason::Normal)
        .expect("normal worker teardown should succeed");
    assert!(runtime.is_alive(caller));
    assert!(runtime
        .failure_snapshot(caller)
        .expect("caller relationships should remain inspectable")
        .links
        .is_empty());
}

#[test]
fn stdlib_pool_rejects_unavailable_workers_without_partial_spawn() {
    let mut scheduler = two_node_scheduler();
    assert_eq!(
        scheduler
            .refresh_membership([
                node("node-a", VmClusterNodeState::Unreachable),
                node("node-b", VmClusterNodeState::Unreachable),
            ])
            .expect_err("membership without active workers must be rejected"),
        "error[vm_distributed_scheduler]: no active nodes available"
    );
    assert_eq!(scheduler.active_node_count(), 2);
    assert_eq!(scheduler.placement_assignment("orphan-worker"), None);
    assert_eq!(
        scheduler
            .place("recovery-worker", &VmPlacementPolicy::LeastConnections)
            .expect("failed membership refresh must preserve the prior pool"),
        decision("recovery-worker", "node-a", "least_connections", false)
    );

    let mut runtime = VmActorRuntime::default();
    assert_eq!(
        runtime
            .spawn_child_with_options(
                VmProcessId::from_raw_for_test(404),
                source("orphan"),
                VmActorSpawnOptions::default().linked(),
            )
            .expect_err("missing caller must reject linked spawn"),
        "cannot spawn child from missing process 404"
    );
    assert!(runtime.live_process_ids().is_empty());
}
