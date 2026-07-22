use super::{
    VmExitReason, VmProcessId, VmProcessRegistryError, VmProcessSource, VmProcessState,
    VmProcessTable,
};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn process_table_allocates_monotonic_process_ids() {
    let mut table = VmProcessTable::default();

    let root = table.spawn_root(source("main"));
    let child = table
        .spawn_child(root, source("worker"))
        .expect("child process should spawn");

    assert_eq!(root.as_u64(), 1);
    assert_eq!(child.as_u64(), 2);
    assert_eq!(
        table.get(child).expect("child should exist").parent,
        Some(root)
    );
}

#[test]
fn process_table_rejects_exited_parent_without_allocating_child_identity() {
    let mut table = VmProcessTable::default();
    let parent = table.spawn_root(source("parent"));
    table
        .exit_process(parent, VmExitReason::Normal)
        .expect("parent should exit");

    assert_eq!(
        table
            .spawn_child(parent, source("orphan"))
            .expect_err("exited parent should fail"),
        "parent process 1 has exited"
    );

    let next = table.spawn_root(source("next"));
    assert_eq!(next.as_u64(), 2);
    assert_eq!(table.metrics().total_processes, 2);
}

#[test]
fn process_table_sends_ordered_messages_and_wakes_recipient() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));
    table
        .get_mut(recipient)
        .expect("recipient should exist")
        .block();

    table
        .send(sender, recipient, ReplValue::String("one".to_string()))
        .expect("first send should succeed");
    table
        .send(sender, recipient, ReplValue::String("two".to_string()))
        .expect("second send should succeed");

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    assert_eq!(recipient_process.state, VmProcessState::Runnable);
    assert_eq!(recipient_process.mailbox_len(), 2);
    assert_eq!(
        recipient_process
            .receive_next()
            .expect("first message")
            .payload,
        ReplValue::String("one".to_string())
    );
    assert_eq!(
        recipient_process
            .receive_next()
            .expect("second message")
            .payload,
        ReplValue::String("two".to_string())
    );
}

#[test]
fn process_mailbox_accounted_bytes_rejects_overflow() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));
    table
        .send_accounted(sender, recipient, ReplValue::Unit, usize::MAX)
        .expect("first accounted message");
    table
        .send_accounted(sender, recipient, ReplValue::Unit, 1)
        .expect("second accounted message");

    assert_eq!(
        table
            .get(recipient)
            .expect("recipient")
            .mailbox_accounted_bytes()
            .expect_err("mailbox bytes should overflow"),
        "process 2 mailbox accounted byte overflow"
    );
}

#[test]
fn process_table_registers_names_idempotently_rejects_conflicts_and_cleans_on_exit() {
    let mut table = VmProcessTable::default();
    let first = table.spawn_root(source("first"));
    let second = table.spawn_root(source("second"));

    table
        .register_name("counter", first)
        .expect("first registration should succeed");
    table
        .register_name("counter", first)
        .expect("same registration should be idempotent");
    let conflict = table
        .register_name("counter", second)
        .expect_err("conflicting registration should fail");

    assert_eq!(table.lookup_name("counter"), Some(first));
    assert_eq!(
        conflict,
        VmProcessRegistryError::Conflict {
            name: "counter".to_string(),
            existing: first,
        }
    );

    table
        .exit_process(first, VmExitReason::Normal)
        .expect("named process should exit");

    assert_eq!(table.lookup_name("counter"), None);
    table
        .register_name("counter", second)
        .expect("name should be reusable after owner exit");
    assert_eq!(table.lookup_name("counter"), Some(second));
}

#[test]
fn process_table_rejects_empty_missing_and_exited_registry_targets() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        table
            .register_name("  ", pid)
            .expect_err("empty process name should fail"),
        VmProcessRegistryError::EmptyName
    );
    assert_eq!(
        table
            .register_name("missing", missing)
            .expect_err("missing process should fail"),
        VmProcessRegistryError::MissingProcess(missing)
    );

    table
        .exit_process(pid, VmExitReason::Killed)
        .expect("process should exit");

    assert_eq!(
        table
            .register_name("dead", pid)
            .expect_err("exited process should fail"),
        VmProcessRegistryError::ExitedProcess(pid)
    );
}

#[test]
fn process_selective_receive_preserves_skipped_messages() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));

    table
        .send(sender, recipient, ReplValue::String("skip".to_string()))
        .expect("skip message should send");
    table
        .send(sender, recipient, ReplValue::Int(42))
        .expect("matching message should send");

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    let selected = recipient_process
        .selective_receive(|message| message.payload == ReplValue::Int(42))
        .expect("integer message should be selected");

    assert_eq!(selected.payload, ReplValue::Int(42));
    assert_eq!(recipient_process.mailbox_len(), 1);
    assert_eq!(
        recipient_process
            .receive_next()
            .expect("skipped message remains")
            .payload,
        ReplValue::String("skip".to_string())
    );
}

#[test]
fn process_selective_receive_commits_middle_match_without_reordering_neighbors() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));

    for value in [1, 2, 3] {
        table
            .send(sender, recipient, ReplValue::Int(value))
            .expect("message should send");
    }

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    let selected = recipient_process
        .selective_receive(|message| message.payload == ReplValue::Int(2))
        .expect("middle message should be selected");

    assert_eq!(selected.id, 2);
    assert_eq!(selected.payload, ReplValue::Int(2));
    assert_eq!(recipient_process.mailbox_len(), 2);
    assert_eq!(
        recipient_process
            .receive_next()
            .expect("left neighbor should remain first")
            .payload,
        ReplValue::Int(1)
    );
    assert_eq!(
        recipient_process
            .receive_next()
            .expect("right neighbor should remain second")
            .payload,
        ReplValue::Int(3)
    );
}

#[test]
fn process_selective_receive_preserves_large_skipped_mailbox_prefix() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));

    for value in 0..256 {
        table
            .send(sender, recipient, ReplValue::Int(value))
            .expect("message should send");
    }
    table
        .send(sender, recipient, ReplValue::Atom("target".to_string()))
        .expect("target message should send");

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    let selected = recipient_process
        .selective_receive(|message| message.payload == ReplValue::Atom("target".to_string()))
        .expect("target message should be selected");

    assert_eq!(selected.id, 257);
    assert_eq!(selected.sender, sender);
    assert_eq!(selected.payload, ReplValue::Atom("target".to_string()));
    assert_eq!(recipient_process.mailbox_len(), 256);

    for expected in 0..256 {
        let message = recipient_process
            .receive_next()
            .expect("skipped message should remain queued");
        assert_eq!(message.id, (expected + 1) as u64);
        assert_eq!(message.sender, sender);
        assert_eq!(message.payload, ReplValue::Int(expected));
    }
    assert_eq!(recipient_process.mailbox_len(), 0);
}

#[test]
fn process_exit_clears_mailbox_and_returns_resource_handles() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));
    table
        .send(sender, recipient, ReplValue::Atom("hello".to_string()))
        .expect("message should send");
    let process = table.get_mut(recipient).expect("recipient should exist");
    process.add_resource_handle("postgres:1");
    process.add_resource_handle("vector:2");

    let cleanup = table
        .exit_process(recipient, VmExitReason::Killed)
        .expect("exit should succeed");

    let process = table
        .get(recipient)
        .expect("recipient should still be inspectable");
    assert_eq!(
        cleanup,
        vec!["postgres:1".to_string(), "vector:2".to_string()]
    );
    assert_eq!(process.mailbox_len(), 0);
    assert_eq!(process.state, VmProcessState::Exited(VmExitReason::Killed));
    assert!(process.resource_handles.is_empty());
}

#[test]
fn process_table_rejects_missing_recipient() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let missing = table
        .spawn_child(sender, source("temporary"))
        .expect("temporary should spawn");
    table
        .exit_process(missing, VmExitReason::Normal)
        .expect("temporary should exit");

    let error = table
        .send(sender, missing, ReplValue::Unit)
        .expect_err("send to exited process should fail");

    assert_eq!(error, "recipient process 2 has exited");
}

#[test]
fn process_table_rejects_missing_parent_sender_recipient_and_exit_pid() {
    let mut table = VmProcessTable::default();
    let root = table.spawn_root(source("root"));
    let missing = VmProcessId(99);

    let missing_parent_error = table
        .spawn_child(missing, source("orphan"))
        .expect_err("missing parent should fail");
    let missing_sender_error = table
        .send(missing, root, ReplValue::Unit)
        .expect_err("missing sender should fail");
    let missing_recipient_error = table
        .send(root, missing, ReplValue::Unit)
        .expect_err("missing recipient should fail");
    let missing_exit_error = table
        .exit_process(missing, VmExitReason::Normal)
        .expect_err("missing process exit should fail");

    assert_eq!(missing_parent_error, "missing parent process 99");
    assert_eq!(missing_sender_error, "missing sender process 99");
    assert_eq!(missing_recipient_error, "missing recipient process 99");
    assert_eq!(missing_exit_error, "missing process 99");
}

#[test]
fn process_table_system_messages_allow_exited_origin_but_reject_missing_origin() {
    let mut table = VmProcessTable::default();
    let origin = table.spawn_root(source("origin"));
    let recipient = table.spawn_root(source("recipient"));
    let missing = VmProcessId(99);
    table
        .exit_process(origin, VmExitReason::Normal)
        .expect("origin should exit");

    assert_eq!(
        table
            .send(origin, recipient, ReplValue::Unit)
            .expect_err("ordinary exited send should fail"),
        "sender process 1 has exited"
    );
    assert_eq!(
        table
            .send_system_message(origin, recipient, ReplValue::Atom("exit".to_string()))
            .expect("system message should preserve exited origin"),
        1
    );
    assert_eq!(
        table
            .send_system_message(missing, recipient, ReplValue::Unit)
            .expect_err("missing system origin should fail"),
        "missing system message origin 99"
    );
    let message = table
        .get_mut(recipient)
        .expect("recipient should exist")
        .receive_next()
        .expect("system message should arrive");
    assert_eq!(message.sender, origin);
    assert_eq!(message.payload, ReplValue::Atom("exit".to_string()));
}

#[test]
fn process_selective_receive_returns_none_when_no_message_matches() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));
    table
        .send(sender, recipient, ReplValue::String("left".to_string()))
        .expect("message should send");

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    let selected =
        recipient_process.selective_receive(|message| message.payload == ReplValue::Int(1));

    assert_eq!(selected, None);
    assert_eq!(recipient_process.mailbox_len(), 1);
}

#[test]
fn process_messages_record_stable_id_and_sender() {
    let mut table = VmProcessTable::default();
    let sender = table.spawn_root(source("sender"));
    let recipient = table.spawn_root(source("recipient"));

    let first = table
        .send(sender, recipient, ReplValue::Atom("one".to_string()))
        .expect("first send should succeed");
    let second = table
        .send(sender, recipient, ReplValue::Atom("two".to_string()))
        .expect("second send should succeed");

    let recipient_process = table.get_mut(recipient).expect("recipient should exist");
    let first_message = recipient_process.receive_next().expect("first message");
    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(first_message.id, 1);
    assert_eq!(first_message.sender, sender);
    assert_eq!(first_message.accounted_bytes, 0);
}

#[test]
fn process_resource_removal_cancellation_and_reduction_accounting_are_stable() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let process = table.get_mut(pid).expect("process should exist");

    process.add_resource_handle("postgres:1");
    process.add_resource_handle("vector:2");
    process.remove_resource_handle("postgres:1");
    process.remove_resource_handle("missing:3");
    process.request_cancellation();
    process.charge_reductions(10);
    process.charge_reductions(u64::MAX);

    assert_eq!(process.resource_handles, vec!["vector:2".to_string()]);
    assert!(process.cancellation_requested);
    assert_eq!(process.reductions, u64::MAX);
}

#[test]
fn process_table_metrics_classify_live_and_released_ownership() {
    let mut processes = VmProcessTable::default();
    let first = processes.spawn_root(source("first"));
    let second = processes.spawn_root(source("second"));
    processes
        .get_mut(first)
        .expect("first process")
        .add_resource_handle("socket:1");
    processes.get_mut(first).expect("first process").heap_bytes = 64;

    let active = processes.metrics();
    assert_eq!(active.total_processes, 2);
    assert_eq!(active.live_processes, 2);
    assert_eq!(active.exited_processes, 0);
    assert_eq!(active.heap_bytes, 64);
    assert_eq!(active.resource_handles, 1);

    assert_eq!(
        processes
            .exit_process(first, VmExitReason::Normal)
            .expect("exit first"),
        vec!["socket:1".to_string()]
    );
    processes
        .exit_process(second, VmExitReason::Killed)
        .expect("exit second");
    let released = processes.metrics();
    assert_eq!(released.live_processes, 0);
    assert_eq!(released.exited_processes, 2);
    assert_eq!(released.heap_bytes, 0);
    assert_eq!(released.resource_handles, 0);
}

#[test]
fn process_block_wake_are_noops_for_nonmatching_states() {
    let mut table = VmProcessTable::default();
    let pid = table.spawn_root(source("worker"));
    let process = table.get_mut(pid).expect("process should exist");

    process.wake();
    assert_eq!(process.state, VmProcessState::Runnable);

    process.block();
    process.block();
    assert_eq!(process.state, VmProcessState::Blocked);

    let cleanup = process.exit(VmExitReason::Error("boom".to_string()));
    process.block();
    process.wake();

    assert!(cleanup.is_empty());
    assert_eq!(
        process.state,
        VmProcessState::Exited(VmExitReason::Error("boom".to_string()))
    );
}

#[test]
fn process_id_test_constructor_preserves_raw_value_for_adversarial_paths() {
    assert_eq!(VmProcessId::system_runtime_worker().as_u64(), 0);
    assert_eq!(VmProcessId::from_raw_for_test(42).as_u64(), 42);
}
