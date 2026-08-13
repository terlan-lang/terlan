use super::*;

#[test]
pub(super) fn supervision_system_propagates_child_supervisor_failure_to_parent_snapshot() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let parent = supervision.create_supervisor("root");
    let child_supervisor = supervision
        .create_child_supervisor_with_policy(parent, "worker-pool", VmRestartPolicy::OneForOne)
        .expect("child supervisor should be created");
    let child = supervision
        .start_child(
            &mut processes,
            child_supervisor,
            VmChildSpec::new("worker", source("worker"), 0),
        )
        .expect("child should start");

    let limited = supervision
        .restart_child(
            &mut processes,
            child_supervisor,
            "worker",
            VmExitReason::Killed,
        )
        .expect("limit result should return");
    let child_snapshot = supervision
        .snapshot(child_supervisor)
        .expect("child supervisor snapshot should be available");
    let parent_snapshot = supervision
        .snapshot(parent)
        .expect("parent snapshot should be available");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: child,
            restart_count: 0
        }
    );
    assert_eq!(child_snapshot.parent_id, Some(parent));
    assert_eq!(
        child_snapshot.state,
        VmSupervisorState::Failed {
            child_id: "worker".to_string(),
            pid: child,
            reason: VmExitReason::Killed
        }
    );
    assert_eq!(
        parent_snapshot.state,
        VmSupervisorState::ChildSupervisorFailed {
            supervisor_id: child_supervisor,
            reason: VmExitReason::Killed
        }
    );
}

#[test]
pub(super) fn supervision_system_tolerates_removed_parent_during_child_failure() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let parent = supervision.create_supervisor("root");
    let child_supervisor = supervision
        .create_child_supervisor_with_policy(parent, "worker-pool", VmRestartPolicy::OneForOne)
        .expect("child supervisor should be created");
    supervision
        .start_child(
            &mut processes,
            child_supervisor,
            VmChildSpec::new("worker", source("worker"), 0),
        )
        .expect("child should start");
    supervision.supervisors.remove(&parent);

    assert!(matches!(
        supervision
            .restart_child(
                &mut processes,
                child_supervisor,
                "worker",
                VmExitReason::Killed,
            )
            .expect("orphaned child failure should remain deterministic"),
        VmSupervisionRestart::LimitReached { .. }
    ));
}

#[test]
pub(super) fn supervision_system_records_restart_history_for_restart_and_limit() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 1)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(10, 40))
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(25)),
        )
        .expect("child should start");

    let first_restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("first restart should succeed");
    let (_, restarted_pid, _) = restarted_event(&first_restart).expect("expected restart");
    let limited = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("limit result should return");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        limited,
        VmSupervisionRestart::LimitReached {
            pid: restarted_pid,
            restart_count: 1
        }
    );
    assert_eq!(snapshot.restart_history.len(), 2);
    assert_eq!(snapshot.restart_history[0].child_id, "worker");
    assert_eq!(snapshot.restart_history[0].old_pid, child);
    assert_eq!(snapshot.restart_history[0].new_pid, Some(restarted_pid));
    assert_eq!(snapshot.restart_history[0].restart_count, 1);
    assert_eq!(
        snapshot.restart_history[0].outcome,
        VmSupervisorRestartHistoryOutcome::Restarted
    );
    assert_eq!(snapshot.restart_history[0].restart_delay_ms, 10);
    assert_eq!(snapshot.restart_history[0].shutdown_timeout_ms, Some(25));
    assert_eq!(snapshot.restart_history[1].old_pid, restarted_pid);
    assert_eq!(snapshot.restart_history[1].new_pid, None);
    assert_eq!(
        snapshot.restart_history[1].outcome,
        VmSupervisorRestartHistoryOutcome::LimitReached
    );
    assert_eq!(snapshot.restart_history[1].restart_count, 1);
}

#[test]
pub(super) fn supervision_system_records_restart_history_for_non_restartable_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 3)
                .with_restart_class(VmChildRestartClass::Temporary)
                .with_shutdown_timeout(VmShutdownTimeout::milliseconds(20)),
        )
        .expect("child should start");

    let not_restarted = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("non-restartable result should return");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should be available");

    assert_eq!(
        not_restarted,
        VmSupervisionRestart::NotRestarted {
            pid: child,
            restart_class: VmChildRestartClass::Temporary,
            reason: VmExitReason::Killed
        }
    );
    assert_eq!(snapshot.restart_history.len(), 1);
    assert_eq!(snapshot.restart_history[0].child_id, "worker");
    assert_eq!(snapshot.restart_history[0].old_pid, child);
    assert_eq!(snapshot.restart_history[0].new_pid, None);
    assert_eq!(snapshot.restart_history[0].reason, VmExitReason::Killed);
    assert_eq!(
        snapshot.restart_history[0].outcome,
        VmSupervisorRestartHistoryOutcome::NotRestarted
    );
    assert_eq!(snapshot.restart_history[0].restart_count, 0);
    assert_eq!(snapshot.restart_history[0].shutdown_timeout_ms, Some(20));
}

#[test]
pub(super) fn supervision_system_reports_missing_child_diagnostic() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");

    let error = supervision
        .restart_child(&mut processes, supervisor, "missing", VmExitReason::Normal)
        .expect_err("missing child should fail");

    assert_eq!(error, "missing child `missing`");
}

#[test]
pub(super) fn supervision_system_reports_missing_supervisor_diagnostic() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let known = supervision.create_supervisor("known");
    let missing = super::super::VmSupervisorId(known.as_u64() + 1);

    let error = supervision
        .start_child(
            &mut processes,
            missing,
            VmChildSpec::new("worker", source("worker"), 1),
        )
        .expect_err("missing supervisor should fail");

    let probe = processes.spawn_root(source("probe"));
    assert_eq!(error, "missing supervisor 2");
    assert!(matches!(
        processes.get(probe).map(|process| &process.state),
        Some(VmProcessState::Runnable)
    ));
}

#[test]
pub(super) fn supervision_system_rejects_missing_child_supervisor_parent() {
    let mut supervision = VmSupervisionSystem::default();
    let missing = super::super::VmSupervisorId(99);

    let error = supervision
        .create_child_supervisor_with_policy(missing, "child", VmRestartPolicy::OneForOne)
        .expect_err("missing parent should fail");

    assert_eq!(error, "missing supervisor 99");
}

#[test]
pub(super) fn supervision_group_policies_report_missing_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    for policy in [VmRestartPolicy::OneForAll, VmRestartPolicy::RestForOne] {
        let supervisor = supervision.create_supervisor_with_policy("root", policy);
        let error = supervision
            .restart_child(&mut processes, supervisor, "missing", VmExitReason::Normal)
            .expect_err("missing child should fail");
        assert_eq!(error, "missing child `missing`");
    }
}

#[test]
pub(super) fn supervision_system_rejects_duplicate_child_id() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("first"), 1),
        )
        .expect("first child should start");

    let error = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("second"), 1),
        )
        .expect_err("duplicate child should fail");

    assert_eq!(error, "child `worker` already exists");
}

#[test]
pub(super) fn supervision_system_restart_exits_live_child_before_restarting() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let old_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 2),
        )
        .expect("child should start");

    let restart = supervision
        .restart_child(&mut processes, supervisor, "worker", VmExitReason::Killed)
        .expect("restart should succeed");

    let (restarted_old_pid, new_pid, restart_count) =
        restarted_event(&restart).expect("expected restart");
    assert_eq!(restarted_old_pid, old_pid);
    assert_eq!(restart_count, 1);
    assert_eq!(
        processes
            .get(old_pid)
            .expect("old child should exist")
            .state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        processes
            .get(new_pid)
            .expect("new child should exist")
            .state,
        VmProcessState::Runnable
    );
}

#[test]
pub(super) fn supervision_system_reports_missing_process_instead_of_panicking_on_restart() {
    let mut processes = VmProcessTable::default();
    let mut empty_processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let old_pid = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 2),
        )
        .expect("child should start");

    let error = supervision
        .restart_child(
            &mut empty_processes,
            supervisor,
            "worker",
            VmExitReason::Killed,
        )
        .expect_err("missing process should fail without panic");
    let snapshot = supervision
        .snapshot(supervisor)
        .expect("snapshot should remain available");

    assert_eq!(error, "missing process 1");
    assert_eq!(snapshot.children.len(), 1);
    assert_eq!(snapshot.children[0].pid, old_pid);
    assert_eq!(snapshot.children[0].restart_count, 0);
}

#[test]
pub(super) fn supervision_system_reports_missing_supervisor_for_restart_and_snapshot() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let known = supervision.create_supervisor("known");
    let missing = super::super::VmSupervisorId(known.as_u64() + 1);

    assert_eq!(
        supervision
            .restart_child(&mut processes, missing, "worker", VmExitReason::Normal)
            .expect_err("missing supervisor restart should fail"),
        "missing supervisor 2"
    );
    assert_eq!(
        supervision
            .snapshot(missing)
            .expect_err("missing supervisor snapshot should fail"),
        "missing supervisor 2"
    );
}
