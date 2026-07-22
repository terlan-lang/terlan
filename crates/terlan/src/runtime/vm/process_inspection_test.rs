use super::{
    VmExitReason, VmProcessId, VmProcessInspectionError, VmProcessSource, VmProcessState,
    VmProcessTable,
};
use crate::runtime::vm::ReplValue;

fn source(name: &str, arity: usize) -> VmProcessSource {
    VmProcessSource::new("app.Worker", name, arity)
}

#[test]
fn process_snapshot_reports_vm_owned_runtime_state() {
    let mut table = VmProcessTable::default();
    let parent = table.spawn_root(source("main", 0));
    let worker = table
        .spawn_child(parent, source("serve", 2))
        .expect("worker should spawn");

    table
        .register_name("worker.secondary", worker)
        .expect("secondary name should register");
    table
        .register_name("worker.primary", worker)
        .expect("primary name should register");
    table
        .send(parent, worker, ReplValue::Int(42))
        .expect("message should send");
    let process = table.get_mut(worker).expect("worker should exist");
    process.charge_reductions(37);
    process.heap_bytes = 4_096;
    process.request_cancellation();
    process.add_resource_handle("tcp:7");
    process.add_resource_handle("file:9");

    let snapshot = table
        .snapshot(worker)
        .expect("worker should be inspectable");

    assert_eq!(snapshot.pid, worker);
    assert_eq!(snapshot.parent, Some(parent));
    assert_eq!(snapshot.source, source("serve", 2));
    assert_eq!(snapshot.state, VmProcessState::Runnable);
    assert_eq!(snapshot.reductions, 37);
    assert_eq!(snapshot.heap_bytes, 4_096);
    assert_eq!(snapshot.mailbox_messages, 1);
    assert!(snapshot.cancellation_requested);
    assert_eq!(snapshot.resource_handles, ["tcp:7", "file:9"]);
    assert_eq!(
        snapshot.registered_names,
        ["worker.primary", "worker.secondary"]
    );
}

#[test]
fn process_snapshot_retains_exit_reason_and_observes_atomic_cleanup() {
    let mut table = VmProcessTable::default();
    let process = table.spawn_root(source("crash", 0));
    table
        .register_name("crashing-worker", process)
        .expect("name should register");
    table
        .send(process, process, ReplValue::Unit)
        .expect("self message should send");
    let process_record = table.get_mut(process).expect("process should exist");
    process_record.heap_bytes = 128;
    process_record.add_resource_handle("socket:1");

    let cleanup = table
        .exit_process(process, VmExitReason::Error("handler failed".to_string()))
        .expect("process should exit");
    let snapshot = table
        .snapshot(process)
        .expect("exited process should remain inspectable");

    assert_eq!(cleanup, ["socket:1"]);
    assert_eq!(
        snapshot.state,
        VmProcessState::Exited(VmExitReason::Error("handler failed".to_string()))
    );
    assert_eq!(snapshot.heap_bytes, 0);
    assert_eq!(snapshot.mailbox_messages, 0);
    assert!(snapshot.resource_handles.is_empty());
    assert!(snapshot.registered_names.is_empty());
}

#[test]
fn process_snapshot_rejects_missing_process_with_typed_identity() {
    let table = VmProcessTable::default();
    let missing = VmProcessId::from_raw_for_test(404);

    assert_eq!(
        table
            .snapshot(missing)
            .expect_err("missing process must not produce a snapshot"),
        VmProcessInspectionError::MissingProcess(missing)
    );
}

#[test]
fn process_snapshots_are_allocation_ordered_and_retain_exited_records() {
    let mut table = VmProcessTable::default();
    let first = table.spawn_root(source("first", 0));
    let second = table.spawn_root(source("second", 1));
    let third = table
        .spawn_child(first, source("third", 2))
        .expect("child should spawn");
    table
        .register_name("second", second)
        .expect("second name should register");
    table
        .exit_process(second, VmExitReason::Killed)
        .expect("second process should exit");

    let snapshots = table.snapshots();

    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.pid)
            .collect::<Vec<_>>(),
        [first, second, third]
    );
    assert_eq!(snapshots[0].source, source("first", 0));
    assert_eq!(
        snapshots[1].state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert!(snapshots[1].registered_names.is_empty());
    assert_eq!(snapshots[2].parent, Some(first));
}

#[test]
fn process_snapshots_are_empty_without_allocated_processes() {
    assert!(VmProcessTable::default().snapshots().is_empty());
}
