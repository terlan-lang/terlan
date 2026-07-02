use super::{VmFailureRuntime, VmMonitorRef};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn failure_runtime_links_processes_idempotently_and_unlinks() {
    let mut table = VmProcessTable::default();
    let left = table.spawn_root(source("left"));
    let right = table.spawn_root(source("right"));
    let mut failure = VmFailureRuntime::default();

    failure
        .link(&table, left, right)
        .expect("link should succeed");
    failure
        .link(&table, right, left)
        .expect("link should be idempotent");

    assert!(failure.is_linked(left, right));
    failure.unlink(left, right);
    assert!(!failure.is_linked(left, right));
}

#[test]
fn failure_runtime_propagates_abnormal_linked_exit() {
    let mut table = VmProcessTable::default();
    let left = table.spawn_root(source("left"));
    let right = table.spawn_root(source("right"));
    table
        .get_mut(right)
        .expect("right should exist")
        .add_resource_handle("vector:1");
    let mut failure = VmFailureRuntime::default();
    failure
        .link(&table, left, right)
        .expect("link should succeed");

    let report = failure
        .exit_process(&mut table, left, VmExitReason::Error("boom".to_string()))
        .expect("exit should propagate");

    assert_eq!(report.exited, vec![left, right]);
    assert_eq!(report.cleanup_handles, vec!["vector:1".to_string()]);
    assert_eq!(
        table.get(right).expect("right should exist").state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
}

#[test]
fn failure_runtime_does_not_propagate_normal_linked_exit() {
    let mut table = VmProcessTable::default();
    let left = table.spawn_root(source("left"));
    let right = table.spawn_root(source("right"));
    let mut failure = VmFailureRuntime::default();
    failure
        .link(&table, left, right)
        .expect("link should succeed");

    let report = failure
        .exit_process(&mut table, left, VmExitReason::Normal)
        .expect("normal exit should not propagate");

    assert_eq!(report.exited, vec![left]);
    assert_eq!(
        table.get(right).expect("right should exist").state,
        VmProcessState::Runnable
    );
}

#[test]
fn failure_runtime_delivers_trapped_exit_message() {
    let mut table = VmProcessTable::default();
    let left = table.spawn_root(source("left"));
    let right = table.spawn_root(source("right"));
    let mut failure = VmFailureRuntime::default();
    failure
        .link(&table, left, right)
        .expect("link should succeed");
    failure
        .set_trap_exits(&table, right, true)
        .expect("trap exit should enable");

    let report = failure
        .exit_process(&mut table, left, VmExitReason::Killed)
        .expect("exit should deliver trapped signal");

    let message = table
        .get_mut(right)
        .expect("right should exist")
        .receive_next()
        .expect("exit signal should be delivered");
    assert_eq!(report.exited, vec![left]);
    assert_eq!(report.delivered_exit_signals, 1);
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(1),
            ReplValue::Atom("killed".to_string()),
        ])
    );
}

#[test]
fn failure_runtime_delivers_monitor_down_message_and_demonitor_suppresses_it() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let first = table.spawn_root(source("first"));
    let second = table.spawn_root(source("second"));
    let mut failure = VmFailureRuntime::default();
    let first_ref = failure
        .monitor(&table, watcher, first)
        .expect("monitor should succeed");
    let second_ref = failure
        .monitor(&table, watcher, second)
        .expect("second monitor should succeed");
    assert_eq!(first_ref, VmMonitorRef(1));
    assert_eq!(second_ref, VmMonitorRef(2));
    assert!(failure.demonitor(second_ref));

    let report = failure
        .exit_process(&mut table, first, VmExitReason::Normal)
        .expect("monitored exit should succeed");
    failure
        .exit_process(&mut table, second, VmExitReason::Normal)
        .expect("demonitored exit should succeed");

    let watcher_process = table.get_mut(watcher).expect("watcher should exist");
    let message = watcher_process
        .receive_next()
        .expect("down message should be delivered");
    assert_eq!(report.delivered_down_messages, 1);
    assert_eq!(watcher_process.mailbox_len(), 0);
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(1),
            ReplValue::Int(2),
            ReplValue::Atom("normal".to_string()),
        ])
    );
}

#[test]
fn failure_runtime_reports_missing_exited_and_self_link_diagnostics() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut failure = VmFailureRuntime::default();

    assert_eq!(
        failure
            .link(&table, pid, pid)
            .expect_err("self link should fail"),
        "cannot link process 1 to itself"
    );
    assert_eq!(
        failure
            .monitor(&table, pid, missing)
            .expect_err("missing monitor target should fail"),
        "cannot monitor missing process 99"
    );
    failure
        .exit_process(&mut table, pid, VmExitReason::Normal)
        .expect("exit should succeed");
    assert_eq!(
        failure
            .set_trap_exits(&table, pid, true)
            .expect_err("exited trap-exit should fail"),
        "cannot set trap exits for exited process 1"
    );
}

#[test]
fn failure_runtime_duplicate_exit_is_noop() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut failure = VmFailureRuntime::default();

    let first = failure
        .exit_process(&mut table, pid, VmExitReason::Normal)
        .expect("first exit should succeed");
    let second = failure
        .exit_process(&mut table, pid, VmExitReason::Killed)
        .expect("second exit should be no-op");

    assert_eq!(first.exited, vec![pid]);
    assert_eq!(second.exited, Vec::new());
}
