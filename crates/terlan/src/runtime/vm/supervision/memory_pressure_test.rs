use super::{
    VmChildSpec, VmRestartPolicy, VmSupervisionMemoryPressure, VmSupervisionRestart,
    VmSupervisionSystem, VmSupervisorState,
};
use crate::runtime::vm::{
    memory::{VmMemoryAccountant, VmMemoryLimits},
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    resource::VmResourceTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn supervision_memory_pressure_continues_or_collects_without_restart() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 1),
        )
        .expect("child");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();

    let accounted = memory
        .account_heap(&mut processes, child, 4)
        .expect("accounted");
    assert_eq!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "worker",
                &accounted,
            )
            .expect("continue"),
        VmSupervisionMemoryPressure::Continue { pid: child }
    );
    let soft = memory
        .account_heap(&mut processes, child, 1)
        .expect("soft pressure");
    assert_eq!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "worker",
                &soft,
            )
            .expect("collect"),
        VmSupervisionMemoryPressure::Collect {
            pid: child,
            projected_bytes: 5,
        }
    );
    assert_eq!(
        processes.get(child).expect("child").state,
        VmProcessState::Runnable
    );
    assert!(supervision
        .snapshot(supervisor)
        .expect("snapshot")
        .restart_history
        .is_empty());
}

#[test]
fn supervision_memory_pressure_rejects_unknown_or_mismatched_ownership() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let missing_supervisor = supervision.create_supervisor("unused");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 1),
        )
        .expect("child");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();
    let pressure = memory
        .account_heap(&mut processes, child, 4)
        .expect("pressure");

    let mut other_system = VmSupervisionSystem::default();
    other_system.create_supervisor("other-root");
    assert_eq!(
        other_system
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                missing_supervisor,
                "worker",
                &pressure,
            )
            .expect_err("unknown supervisor must fail"),
        "missing supervisor 2"
    );
    assert_eq!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "missing",
                &pressure,
            )
            .expect_err("unknown child must fail"),
        "missing child `missing`"
    );

    let mut wrong_owner = pressure.clone();
    wrong_owner.pid = child.as_u64() + 100;
    assert_eq!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "worker",
                &wrong_owner,
            )
            .expect_err("mismatched owner must fail"),
        format!(
            "memory pressure process {} does not match supervised child `worker` process {}",
            wrong_owner.pid,
            child.as_u64()
        )
    );
    assert_eq!(
        processes.get(child).expect("child remains live").state,
        VmProcessState::Runnable
    );
    assert!(supervision
        .snapshot(supervisor)
        .expect("snapshot")
        .restart_history
        .is_empty());
}

#[test]
fn supervision_hard_memory_pressure_cleans_and_restarts_child() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor("root");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 1),
        )
        .expect("child");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();
    memory
        .account_heap(&mut processes, child, 4)
        .expect("initial heap");
    let hard = memory
        .account_heap(&mut processes, child, 5)
        .expect("hard pressure");

    let VmSupervisionMemoryPressure::Restart(VmSupervisionRestart::Restarted {
        old_pid,
        new_pid,
        restart_count,
        ..
    }) = supervision
        .handle_memory_pressure(
            &mut memory,
            &mut resources,
            &mut processes,
            supervisor,
            "worker",
            &hard,
        )
        .expect("restart")
    else {
        panic!("expected memory-pressure restart");
    };
    assert_eq!(old_pid, child);
    assert_eq!(restart_count, 1);
    assert_eq!(
        memory
            .process_metrics(child)
            .expect("metrics")
            .current_bytes,
        0
    );
    assert_eq!(
        processes.get(child).expect("old child").state,
        VmProcessState::Exited(VmExitReason::MemoryLimitExceeded {
            requested_bytes: 5,
            previous_bytes: 4,
            projected_bytes: 9,
        })
    );
    assert_eq!(
        processes.get(new_pid).expect("new child").state,
        VmProcessState::Runnable
    );
}

#[test]
fn supervision_hard_memory_pressure_cleans_one_for_all_group() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 1),
        )
        .expect("first");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 1),
        )
        .expect("second");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();
    memory
        .account_heap(&mut processes, first, 4)
        .expect("first heap");
    memory
        .account_heap(&mut processes, second, 2)
        .expect("second heap");
    let hard = memory
        .account_heap(&mut processes, first, 5)
        .expect("hard pressure");

    let decision = supervision
        .handle_memory_pressure(
            &mut memory,
            &mut resources,
            &mut processes,
            supervisor,
            "first",
            &hard,
        )
        .expect("group restart");
    assert!(matches!(
        decision,
        VmSupervisionMemoryPressure::Restart(VmSupervisionRestart::RestartedGroup { .. })
    ));
    assert_eq!(
        memory
            .process_metrics(first)
            .expect("first metrics")
            .current_bytes,
        0
    );
    assert_eq!(
        memory
            .process_metrics(second)
            .expect("second metrics")
            .current_bytes,
        0
    );
    assert!(matches!(
        processes.get(first).expect("first").state,
        VmProcessState::Exited(_)
    ));
    assert!(matches!(
        processes.get(second).expect("second").state,
        VmProcessState::Exited(_)
    ));
}

#[test]
fn supervision_hard_memory_pressure_cleans_rest_for_one_suffix() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::RestForOne);
    let first = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("first", source("first"), 1),
        )
        .expect("first");
    let second = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("second", source("second"), 1),
        )
        .expect("second");
    let third = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("third", source("third"), 1),
        )
        .expect("third");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();
    memory
        .account_heap(&mut processes, first, 1)
        .expect("first heap");
    memory
        .account_heap(&mut processes, second, 4)
        .expect("second heap");
    memory
        .account_heap(&mut processes, third, 2)
        .expect("third heap");
    let hard = memory
        .account_heap(&mut processes, second, 5)
        .expect("hard pressure");

    assert!(matches!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "second",
                &hard,
            )
            .expect("suffix restart"),
        VmSupervisionMemoryPressure::Restart(VmSupervisionRestart::RestartedGroup { .. })
    ));
    assert_eq!(
        memory
            .process_metrics(first)
            .expect("first metrics")
            .current_bytes,
        1
    );
    assert_eq!(
        memory
            .process_metrics(second)
            .expect("second metrics")
            .current_bytes,
        0
    );
    assert_eq!(
        memory
            .process_metrics(third)
            .expect("third metrics")
            .current_bytes,
        0
    );
    assert_eq!(
        processes.get(first).expect("first").state,
        VmProcessState::Runnable
    );
    assert!(matches!(
        processes.get(second).expect("second").state,
        VmProcessState::Exited(_)
    ));
    assert!(matches!(
        processes.get(third).expect("third").state,
        VmProcessState::Exited(_)
    ));
}

#[test]
fn supervision_hard_memory_pressure_escalates_at_restart_limit() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let parent = supervision.create_supervisor("root");
    let supervisor = supervision
        .create_child_supervisor_with_policy(parent, "pool", VmRestartPolicy::OneForOne)
        .expect("child supervisor");
    let child = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("worker", source("worker"), 0),
        )
        .expect("child");
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut resources = VmResourceTable::default();
    let hard = memory
        .account_heap(&mut processes, child, 9)
        .expect("hard pressure");

    assert!(matches!(
        supervision
            .handle_memory_pressure(
                &mut memory,
                &mut resources,
                &mut processes,
                supervisor,
                "worker",
                &hard,
            )
            .expect("escalation"),
        VmSupervisionMemoryPressure::Restart(VmSupervisionRestart::LimitReached { .. })
    ));
    let reason = VmExitReason::MemoryLimitExceeded {
        requested_bytes: 9,
        previous_bytes: 0,
        projected_bytes: 9,
    };
    assert_eq!(
        supervision.snapshot(supervisor).expect("snapshot").state,
        VmSupervisorState::Failed {
            child_id: "worker".to_string(),
            pid: child,
            reason: reason.clone(),
        }
    );
    assert_eq!(
        supervision.snapshot(parent).expect("parent snapshot").state,
        VmSupervisorState::ChildSupervisorFailed {
            supervisor_id: supervisor,
            reason,
        }
    );
}
