use super::{
    VmSupervisionBackoffCompletion, VmSupervisionBackoffQueue, VmSupervisionBackoffStart,
    VmSupervisionRestartRequest,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::VmScheduler,
    supervision::{
        VmChildSpec, VmRestartBackoffSchedule, VmSupervisionRestart, VmSupervisionSystem,
    },
    timer::VmTimerTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Worker", name, 0)
}

/// Verifies deferred restart diagnostics when supervision ownership disappears.
///
/// Inputs:
/// - A removed supervisor and an existing supervisor without the requested child.
///
/// Output:
/// - Stable missing-supervisor and missing-child errors.
///
/// Transformation:
/// - Exercises the race boundary between a fired backoff deadline and supervisor
///   tree mutation without spawning replacement processes.
#[test]
fn supervision_backoff_restart_rejects_missing_supervisor_and_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let removed = supervision.create_supervisor("removed");
    supervision.supervisors.remove(&removed);

    assert_eq!(
        supervision
            .restart_child_after_backoff(&mut processes, removed, "worker", VmExitReason::Killed,)
            .expect_err("removed supervisor should fail"),
        format!("missing supervisor {}", removed.as_u64())
    );

    let existing = supervision.create_supervisor("existing");
    assert_eq!(
        supervision
            .restart_child_after_backoff(&mut processes, existing, "missing", VmExitReason::Killed,)
            .expect_err("missing child should fail"),
        "missing child `missing`"
    );
}

#[test]
fn supervision_backoff_defers_restart_until_vm_timer_fires() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let failed_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("run"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(10, 40)),
        )
        .expect("child");

    let scheduled = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, 100),
        )
        .expect("schedule restart");
    let VmSupervisionBackoffStart::Deferred {
        restarted_immediately,
        scheduled,
    } = scheduled
    else {
        panic!("restart should be deferred");
    };
    assert!(restarted_immediately.is_empty());
    let [scheduled] = scheduled.as_slice() else {
        panic!("one child restart should have one timer");
    };

    assert_eq!(scheduled.deadline_tick, 110);
    assert_eq!(backoff.pending_len(), 1);
    assert!(matches!(
        processes.get(failed_pid).map(|process| &process.state),
        Some(VmProcessState::Exited(VmExitReason::Killed))
    ));
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 109)
        .is_empty());

    let events = timers.advance_clock(&mut processes, &mut scheduler, 110);
    let completion = backoff
        .handle_timer_event(&mut supervision, &mut processes, &events[0])
        .expect("handle deadline")
        .expect("owned timer event");
    let VmSupervisionBackoffCompletion::Restarted(VmSupervisionRestart::Restarted {
        old_pid,
        new_pid,
        restart_delay_ms,
        ..
    }) = completion
    else {
        panic!("deadline should restart child");
    };
    assert_eq!(events.len(), 1);
    assert_eq!(scheduled.timer_id.as_u64(), 1);
    assert_eq!(old_pid, failed_pid);
    assert_ne!(new_pid, failed_pid);
    assert_eq!(restart_delay_ms, 10);
    assert_eq!(backoff.pending_len(), 0);
}

#[test]
fn supervision_backoff_rejects_duplicate_and_cancels_without_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let failed_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("run"), 2)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(5, 20)),
        )
        .expect("child");
    let VmSupervisionBackoffStart::Deferred { scheduled, .. } = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, 0),
        )
        .expect("schedule")
    else {
        panic!("restart should be deferred");
    };
    let [scheduled] = scheduled.as_slice() else {
        panic!("one child restart should have one timer");
    };
    let timer_id = scheduled.timer_id;

    let duplicate = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, 1),
        )
        .expect_err("duplicate must fail");
    assert!(duplicate.contains("already pending"));
    assert_eq!(
        backoff
            .cancel_restart(&mut supervision, &mut timers, &mut processes, timer_id,)
            .expect("cancel"),
        VmSupervisionBackoffCompletion::Cancelled {
            timer_id,
            failed_pid,
        }
    );
    assert_eq!(backoff.pending_len(), 0);
    assert_eq!(
        supervision.snapshot(supervisor).expect("snapshot").children[0].pid,
        failed_pid
    );
}

#[test]
fn supervision_backoff_ignores_stale_deadline_after_external_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let failed_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("run"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(8, 32)),
        )
        .expect("child");
    backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, 0),
        )
        .expect("schedule");
    let VmSupervisionRestart::Restarted { new_pid, .. } = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("external restart")
    else {
        panic!("external restart should succeed");
    };

    let event = timers
        .advance_clock(&mut processes, &mut scheduler, 8)
        .remove(0);
    assert!(matches!(
        backoff
            .handle_timer_event(&mut supervision, &mut processes, &event)
            .expect("handle")
            .expect("completion"),
        VmSupervisionBackoffCompletion::Stale {
            failed_pid: observed_failed,
            current_pid,
            ..
        } if observed_failed == failed_pid && current_pid == new_pid
    ));
    assert_eq!(backoff.pending_len(), 0);
}

#[test]
fn supervision_backoff_rejects_deadline_overflow_without_exiting_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("run"), 2)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(2, 2)),
        )
        .expect("child");

    let error = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, u64::MAX),
        )
        .expect_err("overflow must fail");
    assert!(error.contains("deadline overflow"));
    assert_eq!(backoff.pending_len(), 0);
    assert_eq!(timers.snapshots(), Vec::new());
    assert!(matches!(
        processes.get(child).map(|process| &process.state),
        Some(VmProcessState::Runnable)
    ));
}

#[test]
fn supervision_backoff_cleans_pending_intent_when_timer_owner_exits() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let failed_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("run"), 2)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(5, 20)),
        )
        .expect("child");
    backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "worker", VmExitReason::Killed, 0),
        )
        .expect("schedule");
    let timer_owner = backoff.timer_owner.expect("timer owner");
    processes
        .exit_process(timer_owner, VmExitReason::Killed)
        .expect("exit timer owner");

    let event = timers
        .advance_clock(&mut processes, &mut scheduler, 5)
        .remove(0);
    assert!(matches!(
        backoff
            .handle_timer_event(&mut supervision, &mut processes, &event)
            .expect("handle")
            .expect("completion"),
        VmSupervisionBackoffCompletion::TimerOwnerExited {
            failed_pid: observed,
            ..
        } if observed == failed_pid
    ));
    assert_eq!(backoff.pending_len(), 0);
}
