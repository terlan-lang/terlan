use super::super::process::{VmProcessResumeState, VmProcessSource, VmProcessState};
use super::super::scheduler::{VmSchedulerClass, VmSchedulerDecision, VmSchedulerOutcome};
use super::VmActorRuntime;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn native_scheduling_reclassifies_owner_before_exact_resume() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-priority"));
    let normal = runtime.spawn_root(source("normal-peer"));
    runtime
        .park_native_continuation(owner.as_u64(), 271, 277)
        .expect("scheduling continuation should park");

    runtime
        .service_native_scheduling(owner.as_u64(), 271, 277, VmSchedulerClass::Priority)
        .expect("native owner should become priority");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    let first = runtime
        .run_next(|process, _| {
            assert_eq!(
                process.pid, owner,
                "priority owner must run before normal peer"
            );
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("priority owner should run");
    assert_eq!(first.pid, Some(owner));
    assert_eq!(first.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(
        runtime.processes().get(normal).expect("normal peer").state,
        VmProcessState::Runnable
    );
}

#[test]
fn native_scheduling_rejects_foreign_owner_without_reclassification() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native-background"));
    let normal = runtime.spawn_root(source("normal-peer"));
    runtime
        .park_native_continuation(owner.as_u64(), 281, 283)
        .expect("scheduling continuation should park");

    assert!(runtime
        .service_native_scheduling(normal.as_u64(), 281, 283, VmSchedulerClass::Priority,)
        .expect_err("foreign scheduling owner must fail")
        .contains("is owned by process"));
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert_eq!(
        runtime.processes().get(owner).expect("parked owner").state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );

    runtime
        .service_native_scheduling(owner.as_u64(), 281, 283, VmSchedulerClass::Background)
        .expect("exact owner should become background");
    let first = runtime
        .run_next(|process, _| {
            assert_eq!(
                process.pid, normal,
                "normal peer must precede background owner"
            );
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("normal peer should run");
    assert_eq!(first.pid, Some(normal));
}
