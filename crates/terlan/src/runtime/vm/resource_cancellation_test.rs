use super::super::process::{VmProcess, VmProcessSource, VmProcessTable};
use super::super::scheduler::{
    VmScheduler, VmSchedulerDecision, VmSchedulerOutcome, VmSchedulerSlice,
};
use super::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy};

/// Builds a stable VM process source for cancellation/resource tests.
fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

/// Returns a benign scheduler decision for tests that prove another scheduler
/// branch prevents slice execution.
fn benign_slice_decision(
    _process: &mut VmProcess,
    _slice: VmSchedulerSlice,
) -> VmSchedulerDecision {
    VmSchedulerDecision::Yield { reductions: 0 }
}

/// Verifies the benign scheduler callback remains a stable no-op decision.
#[test]
fn benign_slice_decision_yields_without_reductions() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let slice = VmSchedulerSlice {
        pid: owner,
        tick: 1,
        reduction_budget: 100,
    };
    let process = processes.get_mut(owner).expect("owner process exists");

    assert_eq!(
        benign_slice_decision(process, slice),
        VmSchedulerDecision::Yield { reductions: 0 }
    );
}

/// Verifies cancellation returns resource handles that become stale after cleanup.
#[test]
fn cancelled_process_resource_cleanup_makes_handles_stale() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut resources = VmResourceTable::default();
    let event = resources
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("native.vector", "users"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("resource registration should succeed");
    let snapshot = resources.snapshots()[0].clone();
    let id = snapshot.id;
    assert_eq!(event, VmResourceEvent::Registered { id, owner });

    let mut scheduler = VmScheduler::default();
    scheduler
        .enqueue_runnable(&processes, owner)
        .expect("owner should enqueue");
    scheduler
        .request_cancellation(&mut processes, owner)
        .expect("owner cancellation should be recorded");

    let cancelled = scheduler
        .run_next(&mut processes, benign_slice_decision)
        .expect("cancellation should terminate the process");

    assert_eq!(
        cancelled.outcome,
        VmSchedulerOutcome::Cancelled(vec![format!("resource:{}", id.as_u64())])
    );
    assert_eq!(
        resources.cleanup_owner(owner),
        vec![VmResourceEvent::CleanedUpOnExit { id, owner }]
    );
    assert_eq!(
        resources
            .get_for_owner(owner, id)
            .expect_err("cancelled owner resource should be stale"),
        format!("stale native resource handle {}", id.as_u64())
    );
    assert!(resources.snapshots().is_empty());
}
