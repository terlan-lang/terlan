use super::{
    VmInternalSupervisionShutdownStart, VmSupervisionShutdownCompletion,
    VmSupervisionShutdownQueue, VmSupervisionShutdownRequest,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::VmScheduler,
    supervision::{
        VmChildRestartClass, VmChildSpec, VmShutdownTimeout, VmSupervisionRestart,
        VmSupervisionSystem,
    },
    timer::VmTimerTable,
    ReplValue,
};

fn source() -> VmProcessSource {
    VmProcessSource::new("app.Worker", "run", 0)
}

fn child_spec(timeout_ms: u64) -> VmChildSpec {
    VmChildSpec::new("worker", source(), 3)
        .with_shutdown_timeout(VmShutdownTimeout::milliseconds(timeout_ms))
}

#[test]
fn supervision_shutdown_waits_for_clean_exit_and_cancels_deadline() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut shutdowns = VmSupervisionShutdownQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(&mut processes, supervisor, child_spec(10))
        .expect("child");

    let start = shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(supervisor, "worker", VmExitReason::Killed, 100),
        )
        .expect("begin shutdown");
    let VmInternalSupervisionShutdownStart::Waiting(scheduled) = start else {
        panic!("configured timeout should defer replacement");
    };
    assert_eq!(scheduled.pid, child);
    assert_eq!(scheduled.deadline_tick, 110);
    assert_eq!(timers.active_count(), 1);
    assert_eq!(shutdowns.pending_len(), 1);
    let message = processes
        .get_mut(child)
        .expect("child process")
        .receive_next()
        .expect("shutdown message");
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("shutdown".to_string()),
            ReplValue::Int(10),
        ])
    );

    processes
        .exit_process(child, VmExitReason::Normal)
        .expect("clean child exit");
    let completion = shutdowns
        .complete_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            supervisor,
            "worker",
        )
        .expect("complete shutdown");
    let VmSupervisionShutdownCompletion::Exited {
        timer_id,
        reason,
        restart: VmSupervisionRestart::Restarted {
            old_pid, new_pid, ..
        },
    } = completion
    else {
        panic!("permanent child should restart after a clean exit");
    };
    assert_eq!(timer_id, scheduled.timer_id);
    assert_eq!(reason, VmExitReason::Normal);
    assert_eq!(old_pid, child);
    assert_ne!(new_pid, child);
    assert_eq!(timers.active_count(), 0);
    assert_eq!(shutdowns.pending_len(), 0);
}

/// Replaces the configurable termination-delay behavior from OTP's
/// `supervisor_2.erl` fixture with deterministic VM logical time.
#[test]
fn supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut shutdowns = VmSupervisionShutdownQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let fast = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("fast", source(), 3)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(20)),
        )
        .expect("fast child");
    let slow = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("slow", source(), 3)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(20)),
        )
        .expect("slow child");

    for child_id in ["fast", "slow"] {
        let start = shutdowns
            .begin_shutdown(
                &mut supervision,
                &mut timers,
                &mut processes,
                VmSupervisionShutdownRequest::new(supervisor, child_id, VmExitReason::Killed, 100),
            )
            .expect("begin child shutdown");
        assert!(matches!(
            start,
            VmInternalSupervisionShutdownStart::Waiting(ref scheduled)
                if scheduled.child_id == child_id && scheduled.deadline_tick == 120
        ));
    }
    assert_eq!(shutdowns.pending_len(), 2);

    let before_deadline = shutdowns
        .advance_clock(
            &mut supervision,
            &mut timers,
            &mut processes,
            &mut scheduler,
            119,
        )
        .expect("advance before shutdown deadline");
    assert!(before_deadline.timer_events.is_empty());
    assert!(before_deadline.completions.is_empty());
    assert_eq!(shutdowns.pending_len(), 2);

    processes
        .exit_process(fast, VmExitReason::Normal)
        .expect("fast child exits within budget");
    let fast_completion = shutdowns
        .complete_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            supervisor,
            "fast",
        )
        .expect("complete fast child shutdown");
    assert!(matches!(
        fast_completion,
        VmSupervisionShutdownCompletion::Exited {
            reason: VmExitReason::Normal,
            restart: VmSupervisionRestart::Restarted { old_pid, .. },
            ..
        } if old_pid == fast
    ));
    assert_eq!(shutdowns.pending_len(), 1);

    let at_deadline = shutdowns
        .advance_clock(
            &mut supervision,
            &mut timers,
            &mut processes,
            &mut scheduler,
            120,
        )
        .expect("advance to shutdown deadline");
    assert!(matches!(
        at_deadline.completions.as_slice(),
        [VmSupervisionShutdownCompletion::TimedOut {
            pid,
            timeout_ms: 20,
            restart: VmSupervisionRestart::Restarted { old_pid, .. },
            ..
        }] if *pid == slow && *old_pid == slow
    ));
    assert_eq!(
        processes.get(slow).map(|process| &process.state),
        Some(&VmProcessState::Exited(VmExitReason::ShutdownTimeout {
            timeout_ms: 20,
        }))
    );
    assert_eq!(timers.active_count(), 0);
    assert_eq!(shutdowns.pending_len(), 0);
}

#[test]
fn supervision_shutdown_deadline_forces_typed_exit_and_restarts_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut shutdowns = VmSupervisionShutdownQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(&mut processes, supervisor, child_spec(10))
        .expect("child");

    shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(supervisor, "worker", VmExitReason::Killed, 100),
        )
        .expect("begin shutdown");
    let foreign_owner = processes.spawn_root(VmProcessSource::new("app.Timer", "wait", 0));
    let foreign_timer = timers
        .start_one_shot(&processes, foreign_owner, 110)
        .expect("foreign timer");
    let advanced = shutdowns
        .advance_clock(
            &mut supervision,
            &mut timers,
            &mut processes,
            &mut scheduler,
            110,
        )
        .expect("advance shutdown clock");
    let [completion] = advanced.completions.as_slice() else {
        panic!("one shutdown deadline should complete");
    };
    let VmSupervisionShutdownCompletion::TimedOut {
        pid,
        timeout_ms,
        restart: VmSupervisionRestart::Restarted {
            old_pid, new_pid, ..
        },
        ..
    } = completion
    else {
        panic!("deadline should force and restart the child");
    };
    assert_eq!(advanced.timer_events.len(), 2);
    assert_eq!(advanced.unhandled_timer_events.len(), 1);
    assert_eq!(advanced.unhandled_timer_events[0].timer_id(), foreign_timer);
    assert_eq!(*pid, child);
    assert_eq!(*timeout_ms, 10);
    assert_eq!(*old_pid, child);
    assert_ne!(*new_pid, child);
    assert_eq!(
        processes.get(child).map(|process| &process.state),
        Some(&VmProcessState::Exited(VmExitReason::ShutdownTimeout {
            timeout_ms: 10,
        }))
    );
    let snapshot = supervision.snapshot(supervisor).expect("snapshot");
    assert_eq!(
        snapshot.restart_history[0].reason,
        VmExitReason::ShutdownTimeout { timeout_ms: 10 }
    );
    assert_eq!(shutdowns.pending_len(), 0);
}

#[test]
fn supervision_shutdown_normal_exit_honors_transient_restart_class() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut shutdowns = VmSupervisionShutdownQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            child_spec(10).with_restart_class(VmChildRestartClass::Transient),
        )
        .expect("child");
    shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(supervisor, "worker", VmExitReason::Killed, 100),
        )
        .expect("begin shutdown");
    processes
        .exit_process(child, VmExitReason::Normal)
        .expect("clean child exit");

    let completion = shutdowns
        .complete_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            supervisor,
            "worker",
        )
        .expect("complete shutdown");
    assert!(matches!(
        completion,
        VmSupervisionShutdownCompletion::Exited {
            reason: VmExitReason::Normal,
            restart: VmSupervisionRestart::NotRestarted {
                pid,
                restart_class: VmChildRestartClass::Transient,
                reason: VmExitReason::Normal,
            },
            ..
        } if pid == child
    ));
}

#[test]
fn supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut shutdowns = VmSupervisionShutdownQueue::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(&mut processes, supervisor, child_spec(10))
        .expect("child");

    let overflow = shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(
                supervisor,
                "worker",
                VmExitReason::Killed,
                u64::MAX - 5,
            ),
        )
        .expect_err("deadline should overflow");
    assert_eq!(
        overflow,
        "supervision shutdown deadline overflow for child `worker` at tick 18446744073709551610"
    );
    assert_eq!(timers.active_count(), 0);
    assert_eq!(shutdowns.pending_len(), 0);
    assert_eq!(processes.get(child).expect("child").mailbox_len(), 0);

    shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(supervisor, "worker", VmExitReason::Killed, 100),
        )
        .expect("begin shutdown");
    let duplicate = shutdowns
        .begin_shutdown(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionShutdownRequest::new(supervisor, "worker", VmExitReason::Killed, 101),
        )
        .expect_err("duplicate should fail");
    assert_eq!(
        duplicate,
        "supervision shutdown for child `worker` is already pending on timer 1"
    );
    assert_eq!(timers.active_count(), 1);
    assert_eq!(shutdowns.pending_len(), 1);
    assert_eq!(processes.get(child).expect("child").mailbox_len(), 1);
}
