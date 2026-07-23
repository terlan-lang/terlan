use super::*;
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::VmScheduler;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

/// Creates one stable process source for queue-owner tests.
fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Multicore", name, 0)
}

/// Returns scheduler identities and owner-matched empty schedulers.
fn schedulers(width: usize) -> (Vec<VmSchedulerId>, Vec<(VmSchedulerId, VmScheduler)>) {
    let topology = VmSchedulerTopology::new(width).expect("topology");
    let identities = topology.schedulers().collect::<Vec<_>>();
    let queues = identities
        .iter()
        .copied()
        .map(|scheduler| {
            (
                scheduler,
                VmScheduler::with_owner(Default::default(), scheduler.owner_word()),
            )
        })
        .collect();
    (identities, queues)
}

/// Uses small deterministic bounds so each cycle has an observable outcome.
fn config(batch: usize) -> VmWorkStealingConfig {
    VmWorkStealingConfig::new(1, batch, 0, 1, 8, [4, 8, 16]).expect("configuration")
}

#[test]
fn owner_threads_move_only_the_bounded_batch_from_live_snapshots() {
    let (identities, mut queues) = schedulers(2);
    let mut processes = VmProcessTable::default();
    for index in 0..6 {
        let process = processes.spawn_root(source(&format!("actor-{index}")));
        queues[0]
            .1
            .enqueue_runnable(&processes, process)
            .expect("queue source actor");
    }
    let mut runtime = VmWorkStealingRuntime::new(processes, queues, config(2)).expect("runtime");
    let before = runtime.snapshots().expect("before snapshots");
    assert_eq!(before[0].runnable_total(), 6);
    assert_eq!(before[1].runnable_total(), 0);

    let cycle = runtime.rebalance(identities[1]).expect("rebalance");
    assert!(matches!(cycle.directive(), VmWorkDirective::Steal(_)));
    assert_eq!(cycle.transferred(), 2);
    let after = runtime.snapshots().expect("after snapshots");
    assert_eq!(after[0].runnable_total(), 4);
    assert_eq!(after[1].runnable_total(), 2);
    runtime.shutdown().expect("shutdown");
}

#[test]
fn destination_collision_returns_every_claim_to_the_victim_owner() {
    let (identities, mut queues) = schedulers(2);
    let mut processes = VmProcessTable::default();
    let first = processes.spawn_root(source("first"));
    let duplicate = processes.spawn_root(source("duplicate"));
    queues[0]
        .1
        .enqueue_runnable(&processes, first)
        .expect("queue first source actor");
    queues[0]
        .1
        .enqueue_runnable(&processes, duplicate)
        .expect("queue duplicate source actor");
    queues[1]
        .1
        .enqueue_runnable(&processes, duplicate)
        .expect("inject destination collision");
    let mut runtime = VmWorkStealingRuntime::new(processes, queues, config(2)).expect("runtime");

    let local = runtime
        .rebalance(identities[1])
        .expect("bounded local service before assistance");
    assert!(matches!(local.directive(), VmWorkDirective::ServeLocal(_)));
    let error = runtime
        .rebalance(identities[1])
        .expect_err("collision must fail closed");
    assert!(error.contains("already has placement"), "{error}");
    let snapshots = runtime.snapshots().expect("rollback snapshots");
    assert_eq!(snapshots[0].runnable_total(), 2);
    assert_eq!(snapshots[1].runnable_total(), 1);
    runtime.shutdown().expect("shutdown");
}

#[test]
fn sleeping_owner_is_woken_once_and_shutdown_rejects_new_cycles() {
    let (identities, queues) = schedulers(2);
    let mut runtime =
        VmWorkStealingRuntime::new(VmProcessTable::default(), queues, config(1)).expect("runtime");
    let cycle = runtime.rebalance(identities[0]).expect("idle cycle");
    assert_eq!(cycle.directive(), VmWorkDirective::Sleep);
    assert!(runtime.publish_runnable(identities[0]).expect("first wake"));
    assert!(!runtime
        .publish_runnable(identities[0])
        .expect("duplicate wake"));

    runtime.shutdown().expect("shutdown");
    assert!(runtime.rebalance(identities[0]).is_err());
    assert!(runtime.snapshots().is_err());
    assert!(runtime.shutdown().is_ok());
}

#[test]
fn constructor_rejects_empty_and_out_of_order_owner_sets() {
    assert!(VmWorkStealingRuntime::new(VmProcessTable::default(), Vec::new(), config(1)).is_err());
    let (identities, mut queues) = schedulers(2);
    queues.swap(0, 1);
    let error = match VmWorkStealingRuntime::new(VmProcessTable::default(), queues, config(1)) {
        Ok(mut runtime) => {
            runtime.shutdown().expect("unexpected runtime shutdown");
            panic!("out-of-order owners must be rejected");
        }
        Err(error) => error,
    };
    assert!(error.contains("occupies slot"), "{error}");
    assert_eq!(identities.len(), 2);
}

#[test]
fn owner_command_capacity_is_finite() {
    assert_eq!(OWNER_COMMAND_CAPACITY, 64);
}
