use super::super::local_trace::{VmLocalTraceConfig, VmLocalTraceEventKind};
use super::super::process::{VmExitReason, VmProcessResumeState, VmProcessSource, VmProcessState};
use super::super::scheduler::{VmSchedulerClass, VmSchedulerDecision, VmSchedulerOutcome};
use super::VmActorRuntime;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("parity.DirtyNative", function, 0)
}

#[test]
fn dirty_bif_suite_native_reclassification_reschedules_without_starving_peers() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native_owner"));
    let peer = runtime.spawn_root(source("peer"));

    runtime
        .park_native_continuation(owner.as_u64(), 101, 103)
        .expect("first native call parks");
    runtime
        .service_native_scheduling(owner.as_u64(), 101, 103, VmSchedulerClass::Background)
        .expect("native call resumes in the requested scheduler class");

    let peer_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, peer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("normal peer runs before background native owner");
    assert_eq!(peer_run.outcome, VmSchedulerOutcome::Blocked);

    let owner_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, owner);
            VmSchedulerDecision::Yield { reductions: 1 }
        })
        .expect("native owner re-enters the VM");
    assert_eq!(owner_run.outcome, VmSchedulerOutcome::Ran);

    runtime
        .park_native_continuation(owner.as_u64(), 107, 109)
        .expect("rescheduled native call parks again");
    runtime
        .service_native_scheduling(owner.as_u64(), 107, 109, VmSchedulerClass::Priority)
        .expect("second native call resumes as priority");
    let priority_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, owner);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("priority owner runs before its normal peer");
    assert_eq!(priority_run.outcome, VmSchedulerOutcome::Blocked);
    assert!(runtime.is_alive(peer));
}

#[test]
fn dirty_bif_suite_parked_process_inspection_registry_and_trace_are_nonblocking() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native_owner"));
    let peer = runtime.spawn_root(source("peer"));
    let native = source("native_wait");
    runtime.enable_local_trace(native.clone(), VmLocalTraceConfig::calls_and_returns());
    let trace_cursor = runtime.local_trace_cursor();
    let call = runtime
        .begin_native_trace_call(owner, native.clone())
        .expect("native entry is traceable");
    runtime
        .park_native_continuation(owner.as_u64(), 113, 127)
        .expect("native owner parks");

    let parked = runtime
        .process_info_snapshot(owner)
        .expect("parked native owner remains inspectable");
    assert_eq!(
        parked.state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    runtime
        .register_name("parity.dirty.native", owner)
        .expect("parked native owner can be registered");
    assert_eq!(runtime.lookup_name("parity.dirty.native"), Some(owner));
    assert_eq!(runtime.pending_native_continuation_count(), 1);

    let peer_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, peer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("inspection and registration never wait for native return");
    assert_eq!(peer_run.outcome, VmSchedulerOutcome::Blocked);
    assert_eq!(runtime.pending_native_continuation_count(), 1);

    runtime
        .resume_native_continuation(owner.as_u64(), 113, 127)
        .expect("native owner resumes through its exact lease");
    runtime
        .complete_native_trace_call(owner, call)
        .expect("native return is traceable");
    let trace = runtime
        .local_trace_since(trace_cursor)
        .expect("native trace remains readable");
    assert_eq!(trace.events.len(), 2);
    assert!(matches!(
        &trace.events[0].kind,
        VmLocalTraceEventKind::Call { location } if location.source == native
    ));
    assert!(matches!(
        &trace.events[1].kind,
        VmLocalTraceEventKind::Return { source, .. } if *source == native
    ));
}

#[test]
fn dirty_bif_suite_exit_during_native_work_hides_identity_and_releases_lease() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("terminating_native_owner"));
    let peer = runtime.spawn_root(source("surviving_peer"));
    runtime
        .register_name("parity.dirty.terminating", owner)
        .expect("register owner before native call");
    runtime
        .park_native_continuation(owner.as_u64(), 131, 137)
        .expect("native owner parks");

    runtime
        .exit_actor(owner, VmExitReason::Killed)
        .expect("native owner exits immediately");
    assert!(!runtime.is_alive(owner));
    assert_eq!(runtime.process_info_snapshot(owner), None);
    assert_eq!(runtime.lookup_name("parity.dirty.terminating"), None);
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .resume_native_continuation(owner.as_u64(), 131, 137)
            .expect_err("late native completion is stale"),
        "stale native continuation 131/137"
    );

    let peer_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, peer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("native termination never blocks the scheduler");
    assert_eq!(peer_run.outcome, VmSchedulerOutcome::Blocked);
}

#[test]
fn dirty_bif_suite_explicit_suspend_survives_native_completion() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("suspended_native_owner"));
    let peer = runtime.spawn_root(source("peer"));
    runtime
        .park_native_continuation(owner.as_u64(), 139, 149)
        .expect("native owner parks");
    runtime
        .suspend(owner)
        .expect("explicit suspension overlays native wait");

    runtime
        .resume_native_continuation(owner.as_u64(), 139, 149)
        .expect("native work completes while explicit suspension remains");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner remains visible")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    let peer_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, peer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("peer progresses while completed owner remains suspended");
    assert_eq!(peer_run.outcome, VmSchedulerOutcome::Blocked);

    runtime
        .resume(owner)
        .expect("explicit resume releases the remaining suspension");
    let owner_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, owner);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("owner runs only after both suspension causes clear");
    assert_eq!(owner_run.outcome, VmSchedulerOutcome::Blocked);
}

#[test]
fn dirty_bif_suite_explicit_resume_cannot_bypass_pending_native_work() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("native_owner"));
    runtime
        .park_native_continuation(owner.as_u64(), 151, 157)
        .expect("native owner parks");
    assert_eq!(
        runtime
            .resume(owner)
            .expect_err("plain resume cannot steal a native continuation"),
        "cannot resume process 1 while native continuation 151/157 is pending"
    );

    runtime
        .suspend(owner)
        .expect("explicit suspension overlays native wait");
    runtime
        .resume(owner)
        .expect("explicit resume clears only its own suspension");
    assert_eq!(
        runtime
            .processes()
            .get(owner)
            .expect("owner remains visible")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    assert_eq!(runtime.scheduled_len(), 0);

    runtime
        .resume_native_continuation(owner.as_u64(), 151, 157)
        .expect("native completion releases the last suspension");
    assert_eq!(
        runtime.processes().get(owner).expect("owner resumes").state,
        VmProcessState::Runnable
    );
    assert_eq!(runtime.scheduled_len(), 1);
}
