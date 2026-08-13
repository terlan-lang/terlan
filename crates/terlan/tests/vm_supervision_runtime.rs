use terlan::runtime::vm::supervision::{
    VmSupervisionChildSpec, VmSupervisionMemoryDecision, VmSupervisionOutcome,
    VmSupervisionRestartStart, VmSupervisionRuntime, VmSupervisionShutdownStart,
    VmSupervisionState, VmSupervisionStrategy,
};

#[test]
fn product_parent_strategy_restarts_failed_and_sibling_supervisor_subtrees() {
    let mut runtime = VmSupervisionRuntime::new("root", VmSupervisionStrategy::OneForAll)
        .expect("product supervision runtime");
    let root = runtime.root();
    let first = runtime
        .create_child_supervisor(root, "first-pool", VmSupervisionStrategy::OneForOne)
        .expect("first child supervisor");
    let second = runtime
        .create_child_supervisor(root, "second-pool", VmSupervisionStrategy::OneForOne)
        .expect("second child supervisor");
    let first_child = runtime
        .start_child(
            first,
            VmSupervisionChildSpec::permanent("first-worker", "app.Worker", "serve", 0, 0),
        )
        .expect("first worker");
    let second_child = runtime
        .start_child(
            second,
            VmSupervisionChildSpec::permanent("second-worker", "app.Worker", "serve", 0, 1),
        )
        .expect("second worker");

    assert!(matches!(
        runtime
            .restart_now(first, "first-worker", "worker crash loop")
            .expect("terminal child result"),
        VmSupervisionOutcome::LimitReached { .. }
    ));
    assert!(matches!(
        runtime.snapshot(root).expect("root snapshot").state,
        VmSupervisionState::ChildSupervisorFailed { .. }
    ));

    let restarted = runtime
        .restart_failed_supervisor(first, "child supervisor failed")
        .expect("parent strategy execution");
    assert_eq!(restarted, vec![first, second]);
    let first_after = runtime.snapshot(first).expect("first after restart");
    let second_after = runtime.snapshot(second).expect("second after restart");
    assert_ne!(first_after.children[0].1, first_child);
    assert_ne!(second_after.children[0].1, second_child);
    assert_eq!(
        runtime.snapshot(root).expect("root after restart").state,
        VmSupervisionState::Running
    );
}

#[test]
fn product_native_boundary_worker_crash_uses_vm_backoff_and_restart() {
    let mut runtime = VmSupervisionRuntime::new("native", VmSupervisionStrategy::OneForOne)
        .expect("product supervision runtime");
    let root = runtime.root();
    let original = runtime
        .start_child(
            root,
            VmSupervisionChildSpec::permanent(
                "native-boundary-worker",
                "runtime.NativeBoundaryWorker",
                "serve",
                0,
                3,
            )
            .with_backoff(5, 20),
        )
        .expect("native worker");

    let VmSupervisionRestartStart::Deferred { deadlines, .. } = runtime
        .schedule_restart(root, "native-boundary-worker", "worker transport lost", 100)
        .expect("deferred restart")
    else {
        panic!("native worker restart should use VM backoff");
    };
    assert_eq!(deadlines.len(), 1);
    assert_eq!(deadlines[0].deadline_tick, 105);
    assert!(runtime
        .advance_restart_clock(104)
        .expect("pre-deadline advance")
        .outcomes
        .is_empty());
    let due = runtime
        .advance_restart_clock(105)
        .expect("restart deadline");
    let VmSupervisionOutcome::Restarted {
        old_child,
        new_child,
        ..
    } = &due.outcomes[0]
    else {
        panic!("deadline should restart native worker");
    };
    assert_eq!(*old_child, original);
    assert_ne!(old_child, new_child);
}

#[test]
fn product_handler_pool_memory_exhaustion_restarts_the_group() {
    let mut runtime = VmSupervisionRuntime::new("http-pool", VmSupervisionStrategy::OneForAll)
        .expect("product supervision runtime");
    let root = runtime.root();
    let first = runtime
        .start_child(
            root,
            VmSupervisionChildSpec::permanent("handler-a", "runtime.HttpHandler", "serve", 0, 2),
        )
        .expect("handler a");
    let second = runtime
        .start_child(
            root,
            VmSupervisionChildSpec::permanent("handler-b", "runtime.HttpHandler", "serve", 0, 2),
        )
        .expect("handler b");

    let VmSupervisionMemoryDecision::Restart(VmSupervisionOutcome::RestartedGroup(restarted)) =
        runtime
            .charge_child_memory(root, "handler-a", 300 * 1024 * 1024)
            .expect("hard limit routes through supervision")
    else {
        panic!("handler pool exhaustion should restart its one-for-all group");
    };
    assert_eq!(restarted.len(), 2);
    assert!(restarted
        .iter()
        .any(|(id, old, new)| id == "handler-a" && *old == first && old != new));
    assert!(restarted
        .iter()
        .any(|(id, old, new)| id == "handler-b" && *old == second && old != new));
}

#[test]
fn product_in_flight_shutdown_timeout_cancels_old_actor_and_restarts() {
    let mut runtime = VmSupervisionRuntime::new("requests", VmSupervisionStrategy::OneForOne)
        .expect("product supervision runtime");
    let root = runtime.root();
    let original = runtime
        .start_child(
            root,
            VmSupervisionChildSpec::permanent(
                "in-flight-request",
                "runtime.HttpRequest",
                "serve",
                0,
                2,
            )
            .with_shutdown_timeout(3),
        )
        .expect("in-flight actor");

    let VmSupervisionShutdownStart::Waiting(deadline) = runtime
        .begin_shutdown(root, "in-flight-request", 10)
        .expect("graceful shutdown")
    else {
        panic!("in-flight request should wait for its shutdown deadline");
    };
    assert_eq!(deadline.deadline_tick, 13);
    assert_eq!(runtime.pending_lifecycle_count(), 1);
    assert!(runtime
        .advance_shutdown_clock(12)
        .expect("pre-deadline advance")
        .outcomes
        .is_empty());
    let due = runtime
        .advance_shutdown_clock(13)
        .expect("shutdown timeout");
    let VmSupervisionOutcome::Restarted {
        old_child,
        new_child,
        shutdown_timeout_ms,
        ..
    } = &due.outcomes[0]
    else {
        panic!("shutdown deadline should force a typed restart");
    };
    assert_eq!(*old_child, original);
    assert_ne!(old_child, new_child);
    assert_eq!(*shutdown_timeout_ms, Some(3));
    assert_eq!(runtime.pending_lifecycle_count(), 0);
}
