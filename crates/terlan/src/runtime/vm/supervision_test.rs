use super::{VmChildSpec, VmRestartPolicy, VmSupervisionRestart, VmSupervisionSystem};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn supervision_system_starts_child_and_exposes_inspection_snapshot() {
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
    assert_eq!(snapshot.name, "root");
    assert_eq!(snapshot.policy, VmRestartPolicy::OneForOne);
    assert_eq!(snapshot.children.len(), 1);
    assert_eq!(snapshot.children[0].child_id, "web");
    assert_eq!(snapshot.children[0].pid, child);
    assert_eq!(snapshot.children[0].restart_limit, 3);
}

#[test]
fn supervision_system_restarts_only_failed_child_for_one_for_one_policy() {
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

    let VmSupervisionRestart::Restarted {
        old_pid,
        new_pid,
        restart_count,
    } = restart
    else {
        panic!("expected restart");
    };
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
fn supervision_system_enforces_restart_limit() {
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
    let VmSupervisionRestart::Restarted { new_pid, .. } = first_restart else {
        panic!("expected first restart");
    };
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
}

#[test]
fn supervision_system_reports_missing_child_diagnostic() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");

    let error = supervision
        .restart_child(&mut processes, supervisor, "missing", VmExitReason::Normal)
        .expect_err("missing child should fail");

    assert_eq!(error, "missing child `missing`");
}

#[test]
fn supervision_system_reports_missing_supervisor_diagnostic() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let known = supervision.create_supervisor("known");
    let missing = super::VmSupervisorId(known.as_u64() + 1);

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
fn supervision_system_rejects_duplicate_child_id() {
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
fn supervision_system_restart_exits_live_child_before_restarting() {
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

    let VmSupervisionRestart::Restarted {
        old_pid: restarted_old_pid,
        new_pid,
        restart_count,
    } = restart
    else {
        panic!("expected restart");
    };
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
fn supervision_system_reports_missing_supervisor_for_restart_and_snapshot() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let known = supervision.create_supervisor("known");
    let missing = super::VmSupervisorId(known.as_u64() + 1);

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
