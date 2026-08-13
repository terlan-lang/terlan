use super::super::{
    VmChildRestartClass, VmChildSpec, VmRestartBackoffSchedule, VmRestartPolicy, VmShutdownTimeout,
    VmSupervisionRestart, VmSupervisionSystem, VmSupervisorState,
};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};

pub(super) fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
pub(super) fn restart_backoff_zero_inputs_disable_delay() {
    assert_eq!(
        VmRestartBackoffSchedule::exponential(0, 10).delay_for_restart_count(2),
        0
    );
    assert_eq!(
        VmRestartBackoffSchedule::exponential(10, 0).delay_for_restart_count(2),
        0
    );
    assert_eq!(
        VmRestartBackoffSchedule::exponential(10, 20).delay_for_restart_count(0),
        0
    );
}

pub(super) fn restarted_event(
    restart: &VmSupervisionRestart,
) -> Option<(VmProcessId, VmProcessId, u32)> {
    match restart {
        VmSupervisionRestart::Restarted {
            old_pid,
            new_pid,
            restart_count,
            ..
        } => Some((*old_pid, *new_pid, *restart_count)),
        _ => None,
    }
}

pub(super) fn restarted_group(
    restart: &VmSupervisionRestart,
) -> Vec<(String, VmProcessId, VmProcessId, u32, u64, Option<u64>)> {
    match restart {
        VmSupervisionRestart::RestartedGroup { restarted } => restarted
            .iter()
            .map(|event| {
                (
                    event.child_id.clone(),
                    event.old_pid,
                    event.new_pid,
                    event.restart_count,
                    event.restart_delay_ms,
                    event.shutdown_timeout_ms,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
pub(super) fn supervision_system_starts_child_and_exposes_inspection_snapshot() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("web", source("web"), 3),
        )
        .expect("child should start");

    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(snapshot.id, supervisor);
    assert_eq!(snapshot.parent_id, None);
    assert_eq!(snapshot.name, "root");
    assert_eq!(snapshot.policy, VmRestartPolicy::OneForOne);
    assert_eq!(snapshot.state, VmSupervisorState::Running);
    assert_eq!(snapshot.children.len(), 1);
    assert!(snapshot.restart_history.is_empty());
    assert_eq!(snapshot.children[0].child_id, "web");
    assert_eq!(snapshot.children[0].pid, child);
    assert_eq!(snapshot.children[0].restart_limit, 3);
}

#[test]
pub(super) fn supervision_system_restarts_only_failed_child_for_one_for_one_policy() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3),
        )
        .expect("second child should start");
    processes
        .exit_process(first, VmExitReason::Error("boom".to_string()))
        .expect("first child should exit");

    let restart = supervision
        .restart_child(
            &mut processes,
            supervisor,
            "first",
            VmExitReason::Error("boom".to_string()),
        )
        .expect("restart should succeed");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    let (old_pid, new_pid, restart_count) = restarted_event(&restart).expect("expected restart");
    assert_eq!(old_pid, first);
    assert_ne!(new_pid, first);
    assert_eq!(restart_count, 1);
    assert!(snapshot
        .children
        .iter()
        .any(|child| child.child_id == "first"
            && child.pid == new_pid
            && child.restart_count == 1));
    assert!(snapshot
        .children
        .iter()
        .any(|child| child.child_id == "second" && child.pid == second));
}

#[test]
pub(super) fn supervision_system_restarts_all_children_for_one_for_all_policy() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3),
        )
        .expect("second child should start");
    processes
        .exit_process(first, VmExitReason::Error("boom".to_string()))
        .expect("first child should exit");

    let restart = supervision
        .restart_child(
            &mut processes,
            supervisor,
            "first",
            VmExitReason::Error("boom".to_string()),
        )
        .expect("restart should succeed");
    let restarted = restarted_group(&restart);
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(restarted.len(), 2);
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "first" && *old_pid == first && *new_pid != first && *count == 1
        }));
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "second" && *old_pid == second && *new_pid != second && *count == 1
        }));
    assert_eq!(
        processes
            .get(second)
            .expect("old second child should exist")
            .state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
    assert_eq!(snapshot.policy, VmRestartPolicy::OneForAll);
    assert!(snapshot
        .children
        .iter()
        .all(|child| child.restart_count == 1
            && child.pid != first
            && child.pid != second
            && child.restart_limit == 3));
}

#[test]
pub(super) fn supervision_system_one_for_all_enforces_restart_limit_before_group_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 0),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3),
        )
        .expect("second child should start");

    let limited = supervision
        .restart_child(&mut processes, supervisor, "first", VmExitReason::Killed)
        .expect("limit result should return");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: first,
            restart_count: 0
        }
    );
    assert_eq!(
        processes.get(first).expect("first child").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        processes.get(second).expect("second child").state,
        VmProcessState::Runnable
    );
    assert!(snapshot
        .children
        .iter()
        .all(|child| child.restart_count == 0));
}

#[test]
pub(super) fn supervision_system_restarts_failed_and_later_children_for_rest_for_one_policy() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::RestForOne);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3),
        )
        .expect("second child should start");
    let third = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("third", source("third"), 3),
        )
        .expect("third child should start");
    processes
        .exit_process(second, VmExitReason::Error("boom".to_string()))
        .expect("second child should exit");

    let restart = supervision
        .restart_child(
            &mut processes,
            supervisor,
            "second",
            VmExitReason::Error("boom".to_string()),
        )
        .expect("restart should succeed");
    let restarted = restarted_group(&restart);
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(restarted.len(), 2);
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "second" && *old_pid == second && *new_pid != second && *count == 1
        }));
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "third" && *old_pid == third && *new_pid != third && *count == 1
        }));
    assert_eq!(
        processes.get(first).expect("first child").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        processes
            .get(third)
            .expect("old third child should exist")
            .state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
    assert!(snapshot
        .children
        .iter()
        .any(|child| child.child_id == "first" && child.pid == first && child.restart_count == 0));
    assert!(snapshot
        .children
        .iter()
        .filter(|child| child.child_id == "second" || child.child_id == "third")
        .all(|child| child.restart_count == 1 && child.pid != second && child.pid != third));
}

#[test]
pub(super) fn supervision_system_rest_for_one_enforces_restart_limit_before_group_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::RestForOne);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 0),
        )
        .expect("second child should start");
    let third = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("third", source("third"), 3),
        )
        .expect("third child should start");

    let limited = supervision
        .restart_child(&mut processes, supervisor, "second", VmExitReason::Killed)
        .expect("limit result should return");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: second,
            restart_count: 0
        }
    );
    assert_eq!(
        processes.get(first).expect("first child").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        processes.get(second).expect("second child").state,
        VmProcessState::Runnable
    );
    assert_eq!(
        processes.get(third).expect("third child").state,
        VmProcessState::Runnable
    );
    assert!(snapshot
        .children
        .iter()
        .all(|child| child.restart_count == 0));
}

#[test]
pub(super) fn supervision_system_temporary_child_never_restarts() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 3)
                .with_restart_class(VmChildRestartClass::Temporary),
        )
        .expect("temporary child should start");

    let restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("temporary child restart should return terminal non-restart outcome");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        restart,
        VmSupervisionRestart::NotRestarted {
            pid: child,
            restart_class: VmChildRestartClass::Temporary,
            reason: VmExitReason::Killed
        }
    );
    assert_eq!(
        processes.get(child).expect("temporary child").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(snapshot.children[0].pid, child);
    assert_eq!(snapshot.children[0].restart_count, 0);
    assert_eq!(
        snapshot.children[0].restart_class,
        VmChildRestartClass::Temporary
    );
}

#[test]
pub(super) fn supervision_system_transient_child_restarts_only_after_abnormal_exit() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let normal_child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("normal", source("normal"), 3)
                .with_restart_class(VmChildRestartClass::Transient),
        )
        .expect("transient normal child should start");
    let abnormal_child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("abnormal", source("abnormal"), 3)
                .with_restart_class(VmChildRestartClass::Transient),
        )
        .expect("transient abnormal child should start");

    let normal_restart = supervision
        .restart_child(&mut processes, supervisor, "normal", VmExitReason::Normal)
        .expect("normal transient child should not restart");
    let abnormal_restart = supervision
        .restart_child(&mut processes, supervisor, "abnormal", VmExitReason::Killed)
        .expect("abnormal transient child should restart");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        normal_restart,
        VmSupervisionRestart::NotRestarted {
            pid: normal_child,
            restart_class: VmChildRestartClass::Transient,
            reason: VmExitReason::Normal
        }
    );
    let (old_pid, new_pid, restart_count) =
        restarted_event(&abnormal_restart).expect("expected abnormal restart");
    assert_eq!(old_pid, abnormal_child);
    assert_ne!(new_pid, abnormal_child);
    assert_eq!(restart_count, 1);
    assert!(snapshot.children.iter().any(|child| {
        child.child_id == "normal" && child.pid == normal_child && child.restart_count == 0
    }));
    assert!(snapshot.children.iter().any(|child| {
        child.child_id == "abnormal" && child.pid == new_pid && child.restart_count == 1
    }));
}

#[test]
pub(super) fn supervision_system_group_restart_skips_non_restartable_children_without_blocking_restartable_siblings(
) {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let permanent = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("permanent", source("permanent"), 3),
        )
        .expect("permanent child should start");
    let temporary = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("temporary", source("temporary"), 3)
                .with_restart_class(VmChildRestartClass::Temporary),
        )
        .expect("temporary child should start");
    let transient = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("transient", source("transient"), 3)
                .with_restart_class(VmChildRestartClass::Transient),
        )
        .expect("transient child should start");

    let restart = supervision
        .restart_child(
            &mut processes,
            supervisor,
            "permanent",
            VmExitReason::Killed,
        )
        .expect("group restart should succeed");
    let restarted = restarted_group(&restart);
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(restarted.len(), 2);
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "permanent" && *old_pid == permanent && *new_pid != permanent && *count == 1
        }));
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, new_pid, count, _, _)| {
            child_id == "transient" && *old_pid == transient && *new_pid != transient && *count == 1
        }));
    assert!(!restarted
        .iter()
        .any(|(child_id, _, _, _, _, _)| child_id == "temporary"));
    assert_eq!(
        processes.get(temporary).expect("temporary child").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert!(snapshot.children.iter().any(|child| {
        child.child_id == "temporary" && child.pid == temporary && child.restart_count == 0
    }));
}

#[test]
pub(super) fn supervision_system_applies_exponential_restart_backoff_for_one_for_one_policy() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 4)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(25, 80)),
        )
        .expect("child should start");

    let first_restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("first restart should succeed");
    let VmSupervisionRestart::Restarted {
        new_pid: second_pid,
        restart_delay_ms: first_delay,
        ..
    } = first_restart
    else {
        panic!("expected first restart");
    };
    let second_restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("second restart should succeed");
    let VmSupervisionRestart::Restarted {
        new_pid: third_pid,
        restart_delay_ms: second_delay,
        ..
    } = second_restart
    else {
        panic!("expected second restart");
    };
    let third_restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("third restart should succeed");
    let VmSupervisionRestart::Restarted {
        restart_delay_ms: third_delay,
        ..
    } = third_restart
    else {
        panic!("expected third restart");
    };
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_ne!(child, second_pid);
    assert_ne!(second_pid, third_pid);
    assert_eq!(first_delay, 25);
    assert_eq!(second_delay, 50);
    assert_eq!(third_delay, 80);
    assert_eq!(snapshot.children[0].restart_count, 3);
    assert_eq!(snapshot.children[0].last_restart_delay_ms, 80);
}

#[test]
pub(super) fn supervision_system_group_restart_reports_per_child_backoff_delays() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(10, 40)),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(5, 20)),
        )
        .expect("second child should start");

    let restart = supervision
        .restart_child(&mut processes, supervisor, "first", VmExitReason::Killed)
        .expect("group restart should succeed");
    let restarted = restarted_group(&restart);
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, _, count, delay, _)| {
            child_id == "first" && *old_pid == first && *count == 1 && *delay == 10
        }));
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, _, count, delay, _)| {
            child_id == "second" && *old_pid == second && *count == 1 && *delay == 5
        }));
    assert!(snapshot
        .children
        .iter()
        .any(|child| child.child_id == "first" && child.last_restart_delay_ms == 10));
    assert!(snapshot
        .children
        .iter()
        .any(|child| child.child_id == "second" && child.last_restart_delay_ms == 5));
}

#[test]
pub(super) fn supervision_system_records_shutdown_timeout_for_live_child_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let old_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 2)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(750)),
        )
        .expect("child should start");

    let restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("restart should succeed");
    let VmSupervisionRestart::Restarted {
        old_pid: restarted_old_pid,
        shutdown_timeout_ms,
        ..
    } = restart
    else {
        panic!("expected restart");
    };
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(restarted_old_pid, old_pid);
    assert_eq!(shutdown_timeout_ms, Some(750));
    assert_eq!(snapshot.children[0].shutdown_timeout_ms, Some(750));
    assert_eq!(snapshot.children[0].last_shutdown_timeout_ms, Some(750));
}

#[test]
pub(super) fn supervision_system_group_restart_reports_per_child_shutdown_timeouts() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 3)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(100)),
        )
        .expect("first child should start");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 3)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(250)),
        )
        .expect("second child should start");

    let restart = supervision
        .restart_child(&mut processes, supervisor, "first", VmExitReason::Killed)
        .expect("group restart should succeed");
    let restarted = restarted_group(&restart);
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, _, count, _, shutdown_timeout)| {
            child_id == "first"
                && *old_pid == first
                && *count == 1
                && *shutdown_timeout == Some(100)
        }));
    assert!(restarted
        .iter()
        .any(|(child_id, old_pid, _, count, _, shutdown_timeout)| {
            child_id == "second"
                && *old_pid == second
                && *count == 1
                && *shutdown_timeout == Some(250)
        }));
    assert!(snapshot.children.iter().any(|child| {
        child.child_id == "first"
            && child.shutdown_timeout_ms == Some(100)
            && child.last_shutdown_timeout_ms == Some(100)
    }));
    assert!(snapshot.children.iter().any(|child| {
        child.child_id == "second"
            && child.shutdown_timeout_ms == Some(250)
            && child.last_shutdown_timeout_ms == Some(250)
    }));
}

#[test]
pub(super) fn supervision_system_enforces_restart_limit() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 1),
        )
        .expect("child should start");
    processes
        .exit_process(child, VmExitReason::Killed)
        .expect("child should exit");
    let first_restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("first restart should succeed");
    let (_, new_pid, _) = restarted_event(&first_restart).expect("expected first restart");
    processes
        .exit_process(new_pid, VmExitReason::Killed)
        .expect("restarted child should exit");

    let limited = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("limit result should return");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: new_pid,
            restart_count: 1
        }
    );
    assert!(restarted_event(&limited).is_none());
}

#[test]
pub(super) fn supervision_system_records_supervisor_failure_when_restart_limit_escalates() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 0),
        )
        .expect("child should start");

    let limited = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("limit result should return");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: child,
            restart_count: 0
        }
    );
    assert_eq!(
        snapshot.state,
        VmSupervisorState::Failed {
            child_id: "worker".to_string(),
            pid: child,
            reason: VmExitReason::Killed
        }
    );
    assert_eq!(snapshot.children[0].pid, child);
    assert_eq!(snapshot.children[0].restart_count, 0);
}
