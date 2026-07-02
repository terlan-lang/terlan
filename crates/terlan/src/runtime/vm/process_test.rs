use super::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};
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
    assert_eq!(VmProcessId::from_raw_for_test(42).as_u64(), 42);
}
