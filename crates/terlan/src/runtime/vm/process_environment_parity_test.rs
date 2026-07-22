use super::{
    VmExitReason, VmProcessId, VmProcessInspectionError, VmProcessSource, VmProcessState,
    VmProcessTable,
};
use crate::runtime::vm::actor::{VmActorDemonitorOptions, VmActorRuntime};
use crate::runtime::vm::ReplValue;

fn source(name: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("app.Environment", name, arity)
}

#[test]
fn process_environment_parity_exposes_typed_lifecycle_state() {
    let mut processes = VmProcessTable::default();
    let parent = processes.spawn_root(source("main", 0));
    let worker = processes
        .spawn_child(parent, source("serve", 2))
        .expect("worker should spawn");
    processes
        .register_name("environment.worker", worker)
        .expect("worker name should register");
    processes
        .send(parent, worker, ReplValue::String("request".to_string()))
        .expect("request should reach worker");

    let worker_process = processes.get_mut(worker).expect("worker should exist");
    worker_process.charge_reductions(17);
    worker_process.heap_bytes = 2_048;
    worker_process
        .enter_execution_frame(source("handle", 1), 4, 9)
        .expect("worker should enter handler frame");

    let before = processes
        .snapshot(worker)
        .expect("live worker should be inspectable");
    assert!(processes.is_alive(worker));
    assert_eq!(before.parent, Some(parent));
    assert_eq!(before.source, source("serve", 2));
    assert_eq!(before.state, VmProcessState::Runnable);
    assert_eq!(before.reductions, 17);
    assert_eq!(before.heap_bytes, 2_048);
    assert_eq!(before.mailbox_messages, 1);
    assert_eq!(before.registered_names, ["environment.worker"]);
    assert_eq!(before.current_location.source, source("handle", 1));
    assert_eq!(before.current_location.instruction_offset, 4);
    assert_eq!(before.current_stacktrace.len(), 2);
    assert_eq!(processes.snapshot(worker).expect("repeat snapshot"), before);

    processes
        .exit_process(worker, VmExitReason::Normal)
        .expect("worker should exit");
    let after = processes
        .snapshot(worker)
        .expect("exited worker should remain inspectable");
    assert!(!processes.is_alive(worker));
    assert_eq!(after.state, VmProcessState::Exited(VmExitReason::Normal));
    assert_eq!(after.heap_bytes, 0);
    assert_eq!(after.mailbox_messages, 0);
    assert!(after.registered_names.is_empty());
    assert_eq!(after.current_stacktrace, before.current_stacktrace);

    let missing = VmProcessId::from_raw_for_test(999_999);
    assert!(!processes.is_alive(missing));
    assert_eq!(
        processes
            .snapshot(missing)
            .expect_err("missing process must not be inspectable"),
        VmProcessInspectionError::MissingProcess(missing)
    );
}

#[test]
fn process_environment_parity_owns_relationships_without_otp_state() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer", 0));
    let target = runtime.spawn_root(source("target", 0));

    assert!(runtime.link_actors(observer, target).expect("link actors"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor target");
    let observer_snapshot = runtime
        .failure_snapshot(observer)
        .expect("inspect observer relationships");
    let target_snapshot = runtime
        .failure_snapshot(target)
        .expect("inspect target relationships");
    assert_eq!(observer_snapshot.links, [target]);
    assert_eq!(observer_snapshot.monitoring.len(), 1);
    assert_eq!(observer_snapshot.monitoring[0].peer, target);
    assert_eq!(observer_snapshot.monitoring[0].monitor_ref, monitor_ref);
    assert_eq!(target_snapshot.links, [observer]);
    assert_eq!(target_snapshot.monitored_by.len(), 1);
    assert_eq!(target_snapshot.monitored_by[0].peer, observer);

    let missing = VmProcessId::from_raw_for_test(999_999);
    let stable_before_rejection = runtime
        .failure_snapshot(observer)
        .expect("snapshot before rejected relationship");
    assert_eq!(
        runtime
            .link_actors(observer, missing)
            .expect_err("missing target must reject link"),
        "cannot link missing process 999999"
    );
    assert_eq!(
        runtime
            .monitor_actor(observer, missing)
            .expect_err("missing target must reject monitor"),
        "cannot monitor missing process 999999"
    );
    assert_eq!(
        runtime
            .failure_snapshot(observer)
            .expect("snapshot after rejected relationships"),
        stable_before_rejection
    );

    assert!(
        runtime
            .demonitor_actor(observer, monitor_ref, VmActorDemonitorOptions::default(),)
            .expect("remove monitor")
            .removed
    );
    assert!(runtime
        .unlink_actors(observer, target)
        .expect("remove link"));
    let cleared = runtime
        .failure_snapshot(observer)
        .expect("inspect cleared relationships");
    assert!(cleared.links.is_empty());
    assert!(cleared.monitoring.is_empty());
}
