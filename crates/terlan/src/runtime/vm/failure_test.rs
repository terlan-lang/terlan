use super::{reason_value, VmFailureRuntime, VmTrapExitError, VmTrapExitUpdate};
use crate::runtime::vm::process::{
    VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable,
};
use crate::runtime::vm::reference::VmReferenceAllocator;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn references() -> VmReferenceAllocator {
    VmReferenceAllocator::new("test-node", 7).expect("test reference namespace")
}

#[test]
fn trap_exit_errors_render_stable_process_identity() {
    let missing = VmProcessId::from_raw_for_test(41);
    let exited = VmProcessId::from_raw_for_test(42);

    assert_eq!(
        VmTrapExitError::MissingProcess(missing).to_string(),
        "cannot inspect trap exits for missing process 41"
    );
    assert_eq!(
        VmTrapExitError::ExitedProcess(exited).to_string(),
        "cannot inspect trap exits for exited process 42"
    );
}

#[test]
fn failure_reason_renders_memory_limit_accounting_without_overflow() {
    assert_eq!(
        reason_value(&VmExitReason::MemoryLimitExceeded {
            requested_bytes: 5,
            previous_bytes: 4,
            projected_bytes: usize::MAX,
        }),
        ReplValue::Tuple(vec![
            ReplValue::Atom("memory_limit_exceeded".to_string()),
            ReplValue::Int(5),
            ReplValue::Int(4),
            ReplValue::Int(i64::MAX),
        ])
    );
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
fn failure_runtime_trap_exit_updates_return_previous_state() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut failure = VmFailureRuntime::default();

    assert!(!failure
        .trap_exits(&table, pid)
        .expect("query should succeed"));
    assert_eq!(
        failure
            .set_trap_exits(&table, pid, true)
            .expect("enable should succeed"),
        VmTrapExitUpdate {
            previous: false,
            current: true,
        }
    );
    assert_eq!(
        failure
            .set_trap_exits(&table, pid, true)
            .expect("repeat enable should succeed"),
        VmTrapExitUpdate {
            previous: true,
            current: true,
        }
    );
    assert!(failure
        .trap_exits(&table, pid)
        .expect("query should succeed"));
    assert_eq!(
        failure
            .set_trap_exits(&table, pid, false)
            .expect("disable should succeed"),
        VmTrapExitUpdate {
            previous: true,
            current: false,
        }
    );
    assert_eq!(failure.trap_exit_process_count(), 0);
}

#[test]
fn failure_runtime_trap_exit_errors_are_typed_and_side_effect_free() {
    let mut table = VmProcessTable::default();
    let live = table.spawn_root(source("live"));
    let exited = table.spawn_root(source("exited"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut failure = VmFailureRuntime::default();
    failure
        .set_trap_exits(&table, live, true)
        .expect("live process should enable trap exits");
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("process should exit");

    assert_eq!(
        failure
            .set_trap_exits(&table, missing, true)
            .expect_err("missing process should fail"),
        VmTrapExitError::MissingProcess(missing)
    );
    assert_eq!(
        failure
            .trap_exits(&table, exited)
            .expect_err("exited process should fail"),
        VmTrapExitError::ExitedProcess(exited)
    );
    assert!(failure
        .trap_exits(&table, live)
        .expect("unrelated state should be retained"));
    assert_eq!(failure.trap_exit_process_count(), 1);
}

#[test]
fn failure_runtime_exit_removes_trap_exit_state() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let mut failure = VmFailureRuntime::default();
    failure
        .set_trap_exits(&table, pid, true)
        .expect("trap exits should enable");

    failure
        .exit_process(&mut table, pid, VmExitReason::Normal)
        .expect("exit should succeed");

    assert_eq!(failure.trap_exit_process_count(), 0);
    assert_eq!(
        failure
            .trap_exits(&table, pid)
            .expect_err("exited process should not be inspectable"),
        VmTrapExitError::ExitedProcess(pid)
    );
}

#[test]
fn failure_runtime_delivers_monitor_down_message_and_demonitor_suppresses_it() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let first = table.spawn_root(source("first"));
    let second = table.spawn_root(source("second"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    let first_ref = failure
        .monitor(&mut references, &table, watcher, first)
        .expect("monitor should succeed");
    let second_ref = failure
        .monitor(&mut references, &table, watcher, second)
        .expect("second monitor should succeed");
    assert_eq!(first_ref.as_u64(), 1);
    assert_eq!(second_ref.as_u64(), 2);
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
fn failure_runtime_target_exit_consumes_monitor_reference() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let target = table.spawn_root(source("target"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    let monitor_ref = failure
        .monitor(&mut references, &table, watcher, target)
        .expect("monitor should register");

    let report = failure
        .exit_process(&mut table, target, VmExitReason::Killed)
        .expect("target exit should succeed");

    assert_eq!(report.delivered_down_messages, 1);
    assert_eq!(failure.monitor_count(), 0);
    assert!(!failure.demonitor(monitor_ref));
}

#[test]
fn failure_runtime_cleans_watcher_monitors_on_exit() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let target = table.spawn_root(source("target"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    let monitor_ref = failure
        .monitor(&mut references, &table, watcher, target)
        .expect("monitor should register");

    assert_eq!(failure.monitor_count(), 1);

    let watcher_report = failure
        .exit_process(&mut table, watcher, VmExitReason::Normal)
        .expect("watcher exit should succeed");

    assert_eq!(watcher_report.exited, vec![watcher]);
    assert_eq!(watcher_report.delivered_down_messages, 0);
    assert_eq!(failure.monitor_count(), 0);
    assert!(!failure.demonitor(monitor_ref));

    let target_report = failure
        .exit_process(&mut table, target, VmExitReason::Killed)
        .expect("target exit should succeed");

    assert_eq!(target_report.exited, vec![target]);
    assert_eq!(target_report.delivered_down_messages, 0);
}

#[test]
fn failure_runtime_reports_missing_exited_and_self_link_diagnostics() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut failure = VmFailureRuntime::default();
    let mut references = references();

    assert_eq!(
        failure
            .link(&table, pid, pid)
            .expect_err("self link should fail"),
        "cannot link process 1 to itself"
    );
    assert_eq!(
        failure
            .monitor(&mut references, &table, pid, missing)
            .expect_err("missing monitor target should fail"),
        "cannot monitor missing process 99"
    );
    failure
        .exit_process(&mut table, pid, VmExitReason::Normal)
        .expect("exit should succeed");
    let live = table.spawn_root(source("live"));
    assert_eq!(
        failure
            .link(&table, live, pid)
            .expect_err("linking an exited process should fail"),
        "cannot link exited process 1"
    );
    assert_eq!(
        failure
            .set_trap_exits(&table, pid, true)
            .expect_err("exited trap-exit should fail"),
        VmTrapExitError::ExitedProcess(pid)
    );
}

#[test]
fn failure_runtime_reports_missing_link_monitor_and_exit_diagnostics() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut failure = VmFailureRuntime::default();
    let mut references = references();

    assert_eq!(
        failure
            .link(&table, missing, pid)
            .expect_err("missing left link pid should fail"),
        "cannot link missing process 99"
    );
    assert_eq!(
        failure
            .link(&table, pid, missing)
            .expect_err("missing right link pid should fail"),
        "cannot link missing process 99"
    );
    assert_eq!(
        failure
            .monitor(&mut references, &table, missing, pid)
            .expect_err("missing watcher should fail"),
        "cannot monitor from missing process 99"
    );
    assert_eq!(
        failure
            .exit_process(&mut table, missing, VmExitReason::Normal)
            .expect_err("missing exit should fail"),
        "cannot exit missing process 99"
    );
}

#[test]
fn failure_runtime_disables_trap_exits_and_skips_dead_link_or_monitor_peer() {
    let mut table = VmProcessTable::default();
    let source_pid = table.spawn_root(source("source"));
    let trap_peer = table.spawn_root(source("trap-peer"));
    let dead_link = table.spawn_root(source("dead-link"));
    let dead_watcher = table.spawn_root(source("dead-watcher"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    failure
        .link(&table, source_pid, trap_peer)
        .expect("trap peer link should succeed");
    failure
        .link(&table, source_pid, dead_link)
        .expect("dead link should succeed");
    failure
        .set_trap_exits(&table, trap_peer, true)
        .expect("trap exits should enable");
    failure
        .set_trap_exits(&table, trap_peer, false)
        .expect("trap exits should disable");
    failure
        .monitor(&mut references, &table, dead_watcher, source_pid)
        .expect("monitor should register");
    table
        .exit_process(dead_link, VmExitReason::Normal)
        .expect("linked peer should exit first");
    table
        .exit_process(dead_watcher, VmExitReason::Normal)
        .expect("watcher should exit first");

    let report = failure
        .exit_process(&mut table, source_pid, VmExitReason::Normal)
        .expect("normal exit should succeed");

    assert_eq!(report.exited, vec![source_pid]);
    assert_eq!(report.delivered_exit_signals, 0);
    assert_eq!(report.delivered_down_messages, 0);
    assert_eq!(
        table
            .get(trap_peer)
            .expect("trap peer should remain live")
            .mailbox_len(),
        0
    );
}

#[test]
fn failure_runtime_handles_right_side_and_unrelated_links() {
    let mut table = VmProcessTable::default();
    let unrelated_left = table.spawn_root(source("unrelated-left"));
    let unrelated_right = table.spawn_root(source("unrelated-right"));
    let left = table.spawn_root(source("left"));
    let right = table.spawn_root(source("right"));
    let mut failure = VmFailureRuntime::default();
    failure
        .link(&table, unrelated_left, unrelated_right)
        .expect("unrelated link should succeed");
    failure
        .link(&table, left, right)
        .expect("target link should succeed");

    let report = failure
        .exit_process(&mut table, right, VmExitReason::Killed)
        .expect("right-side linked exit should propagate");

    assert_eq!(report.exited, vec![right, left]);
    assert!(failure.is_linked(unrelated_left, unrelated_right));
    assert!(!failure.is_linked(left, right));
}

#[test]
fn failure_runtime_reports_error_reason_in_down_message() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let target = table.spawn_root(source("target"));
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    failure
        .monitor(&mut references, &table, watcher, target)
        .expect("monitor should register");

    let report = failure
        .exit_process(&mut table, target, VmExitReason::Error("boom".to_string()))
        .expect("target exit should succeed");

    let message = table
        .get_mut(watcher)
        .expect("watcher should exist")
        .receive_next()
        .expect("down message should be delivered");
    assert_eq!(report.delivered_down_messages, 1);
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(1),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("boom".to_string())
            ])
        ])
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

#[test]
fn ordinary_and_monitor_references_share_sequence_after_validation() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let target = table.spawn_root(source("target"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut failure = VmFailureRuntime::default();
    let mut references = references();
    let ordinary = references.allocate_reference().expect("ordinary reference");

    assert_eq!(
        failure
            .monitor(&mut references, &table, watcher, missing)
            .expect_err("missing target must fail before allocation"),
        "cannot monitor missing process 99"
    );

    let monitor = failure
        .monitor(&mut references, &table, watcher, target)
        .expect("valid monitor");
    let following = references
        .allocate_reference()
        .expect("following reference");

    assert_eq!(ordinary.as_u64(), 1);
    assert_eq!(monitor.as_u64(), 2);
    assert_eq!(monitor.reference().node_id(), "test-node");
    assert_eq!(monitor.reference().epoch(), 7);
    assert_eq!(following.as_u64(), 3);
}

#[test]
fn monitor_reference_exhaustion_preserves_failure_state() {
    let mut table = VmProcessTable::default();
    let watcher = table.spawn_root(source("watcher"));
    let target = table.spawn_root(source("target"));
    let mut failure = VmFailureRuntime::default();
    let mut references =
        VmReferenceAllocator::with_limits("test-node", 7, 0, 1).expect("monitor-disabled runtime");

    assert_eq!(
        failure
            .monitor(&mut references, &table, watcher, target)
            .expect_err("exhausted monitor allocation must fail"),
        "VM reference sequence exhausted"
    );
    assert_eq!(failure.monitor_count(), 0);
}
