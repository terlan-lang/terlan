use super::*;

fn policy(strategy: &str, limit: i64, window: i64) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("supervisor".into()),
        ReplValue::Int(1),
        ReplValue::Atom(strategy.into()),
        ReplValue::Int(limit),
        ReplValue::Int(window),
    ])
}

fn spec(id: i64, order: i64, delay: i64) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("supervisor_child".into()),
        ReplValue::Int(id),
        ReplValue::Int(7),
        ReplValue::Atom("permanent".into()),
        ReplValue::Int(order),
        ReplValue::Int(delay),
        ReplValue::Int(delay),
        ReplValue::Int(0),
    ])
}

fn start(
    runtime: &mut VmSourceSupervision,
    processes: &mut VmProcessTable,
    owner: VmProcessId,
    policy: &ReplValue,
    spec: &ReplValue,
) -> VmProcessId {
    runtime
        .start(
            processes,
            owner,
            policy,
            spec,
            None,
            VmProcessSource::new("fixture.Worker", "spawn_7", 0),
        )
        .unwrap()
}

fn crash(
    runtime: &mut VmSourceSupervision,
    processes: &mut VmProcessTable,
    timers: &mut VmTimerTable,
    pid: VmProcessId,
    now: u64,
) {
    let reason = VmExitReason::Error("fixture crash".into());
    processes.exit_process(pid, reason.clone()).unwrap();
    runtime.record_exit(pid, reason);
    let (root, id, reason, cleanup) = runtime.next_exit(now).unwrap();
    for child in cleanup {
        if !matches!(
            processes.get(child).map(|process| &process.state),
            Some(VmProcessState::Exited(_))
        ) {
            processes.exit_process(child, reason.clone()).unwrap();
        }
    }
    runtime
        .restart(processes, timers, root, &id, reason)
        .unwrap();
}

#[test]
fn rest_for_one_uses_declared_order_and_preserves_earlier_children() {
    let mut runtime = VmSourceSupervision::default();
    let mut processes = VmProcessTable::default();
    let mut timers = VmTimerTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("fixture.Main", "main", 0));
    let policy = policy("rest_for_one", 3, 100);
    let last = start(&mut runtime, &mut processes, owner, &policy, &spec(1, 2, 0));
    let first = start(&mut runtime, &mut processes, owner, &policy, &spec(2, 0, 0));
    let middle = start(&mut runtime, &mut processes, owner, &policy, &spec(3, 1, 0));
    while runtime.take_ready().unwrap().is_some() {}
    crash(&mut runtime, &mut processes, &mut timers, middle, 0);
    assert_eq!(
        runtime
            .current(owner, &policy, &spec(2, 0, 0), &processes)
            .unwrap(),
        first
    );
    assert_ne!(
        runtime
            .current(owner, &policy, &spec(3, 1, 0), &processes)
            .unwrap(),
        middle
    );
    assert_ne!(
        runtime
            .current(owner, &policy, &spec(1, 2, 0), &processes)
            .unwrap(),
        last
    );
    assert!(runtime.take_ready().unwrap().is_some());
    assert!(runtime.take_ready().unwrap().is_some());
    assert!(runtime.take_ready().unwrap().is_none());
}

#[test]
fn stopping_a_pending_child_revokes_its_backoff_restart() {
    let mut runtime = VmSourceSupervision::default();
    let mut processes = VmProcessTable::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = crate::runtime::vm::scheduler::VmScheduler::default();
    let owner = processes.spawn_root(VmProcessSource::new("fixture.Main", "main", 0));
    let policy = policy("one_for_one", 3, 100);
    let declaration = spec(1, 0, 10);
    let child = start(&mut runtime, &mut processes, owner, &policy, &declaration);
    runtime.take_ready().unwrap();
    crash(&mut runtime, &mut processes, &mut timers, child, 0);
    assert!(runtime.take_ready().unwrap().is_none());
    runtime
        .stop(
            owner,
            &policy,
            &declaration,
            &mut processes,
            &mut timers,
            &mut scheduler,
        )
        .unwrap();
    for event in timers.advance_clock(&mut processes, &mut scheduler, 10) {
        runtime.timer_event(&mut processes, &event);
    }
    assert!(runtime.take_ready().unwrap().is_none());
    assert!(runtime
        .current(owner, &policy, &declaration, &processes)
        .is_err());
}

#[test]
fn restart_intensity_expires_at_the_window_boundary() {
    let mut runtime = VmSourceSupervision::default();
    let mut processes = VmProcessTable::default();
    let mut timers = VmTimerTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("fixture.Main", "main", 0));
    let policy = policy("one_for_one", 1, 10);
    let declaration = spec(1, 0, 0);
    let first = start(&mut runtime, &mut processes, owner, &policy, &declaration);
    runtime.take_ready().unwrap();
    crash(&mut runtime, &mut processes, &mut timers, first, 0);
    let second = runtime.take_ready().unwrap().unwrap().pid;
    crash(&mut runtime, &mut processes, &mut timers, second, 10);
    let third = runtime.take_ready().unwrap().unwrap().pid;
    assert_ne!(second, third);
    assert_eq!(
        runtime
            .current(owner, &policy, &declaration, &processes)
            .unwrap(),
        third
    );
}

#[test]
fn owner_exit_revokes_all_children_and_pending_entries() {
    let mut runtime = VmSourceSupervision::default();
    let mut processes = VmProcessTable::default();
    let mut timers = VmTimerTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("fixture.Main", "main", 0));
    let policy = policy("one_for_all", 3, 100);
    let child = start(&mut runtime, &mut processes, owner, &policy, &spec(1, 0, 0));
    assert_eq!(
        runtime
            .owner_exited(owner, &mut processes, &mut timers)
            .unwrap(),
        vec![child]
    );
    assert!(runtime.take_ready().unwrap().is_none());
}

#[test]
fn stopped_reaped_sibling_does_not_break_group_restart() {
    let mut runtime = crate::runtime::vm::actor::VmActorRuntime::default();
    let source = VmProcessSource::new("fixture.Worker", "spawn_7", 0);
    let owner = runtime.spawn_root(source.clone());
    let policy = policy("one_for_all", 3, 100);
    let first = spec(1, 0, 0);
    let second = spec(2, 1, 0);
    let old_first = runtime
        .start_supervised_child(owner, &policy, &first, None, source.clone())
        .unwrap();
    let old_second = runtime
        .start_supervised_child(owner, &policy, &second, None, source)
        .unwrap();
    runtime.take_supervised_start().unwrap();
    runtime.take_supervised_start().unwrap();
    runtime
        .supervised_operation(owner, "stop_child", &[policy.clone(), first])
        .unwrap();
    runtime.reap_exited_actors().unwrap();
    assert!(runtime.processes().get(old_first).is_none());
    runtime
        .exit_actor(old_second, VmExitReason::Error("fixture crash".into()))
        .unwrap();
    let replacement = runtime
        .take_supervised_start()
        .expect("stopped history must not enter restart selection")
        .unwrap();
    assert_ne!(replacement.pid, old_second);
    assert!(runtime.take_supervised_start().unwrap().is_none());
}

#[test]
fn owner_exit_terminates_a_child_already_waiting_for_shutdown() {
    let mut runtime = crate::runtime::vm::actor::VmActorRuntime::default();
    let source = VmProcessSource::new("fixture.Worker", "spawn_7", 0);
    let owner = runtime.spawn_root(source.clone());
    let policy = policy("one_for_one", 3, 100);
    let mut declaration = spec(1, 0, 0);
    if let ReplValue::Tuple(fields) = &mut declaration {
        fields[7] = ReplValue::Int(10);
    }
    let child = runtime
        .start_supervised_child(owner, &policy, &declaration, None, source)
        .unwrap();
    runtime.take_supervised_start().unwrap();
    runtime
        .supervised_operation(owner, "stop_child", &[policy, declaration])
        .unwrap();
    assert!(runtime.is_alive(child));
    runtime.exit_actor(owner, VmExitReason::Normal).unwrap();
    assert!(!runtime.is_alive(child));
    assert!(runtime.take_supervised_start().unwrap().is_none());
}

#[test]
fn source_errors_preserve_configuration_and_lifecycle_categories() {
    let mut runtime = VmSourceSupervision::default();
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("fixture.Main", "main", 0));
    let invalid = policy("unknown", 3, 100);
    let error = runtime
        .start(
            &mut processes,
            owner,
            &invalid,
            &spec(1, 0, 0),
            None,
            VmProcessSource::new("fixture.Worker", "spawn_7", 0),
        )
        .unwrap_err();
    assert_eq!(error.kind(), Configuration);
    assert!(runtime.roots.is_empty());
    let root = policy("one_for_one", 3, 100);
    let child = start(&mut runtime, &mut processes, owner, &root, &spec(1, 0, 0));
    processes.exit_process(child, VmExitReason::Normal).unwrap();
    let error = runtime
        .current(owner, &root, &spec(1, 0, 0), &processes)
        .unwrap_err();
    assert_eq!(error.kind(), ChildLifecycle);
    assert_eq!(error.message(), "child has no running process");
}
