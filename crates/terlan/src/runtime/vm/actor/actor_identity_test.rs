use super::super::super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn actor_context_self_send_preserves_identity_and_wakes_actor() {
    let mut runtime = VmActorRuntime::default();
    let pid = runtime.spawn_root(source("worker"));
    let context = runtime.context(pid).expect("context should be available");
    assert_eq!(context.process_id(), pid);
    assert_eq!(
        runtime
            .receive_next_or_block(pid)
            .expect("empty receive should succeed"),
        VmActorReceive::Blocked
    );

    let message_id = runtime
        .send_self(context, ReplValue::String("wake".to_string()))
        .expect("self send should succeed");

    assert_eq!(message_id, 1);
    assert_eq!(
        runtime
            .processes()
            .get(pid)
            .expect("actor should exist")
            .state,
        VmProcessState::Runnable
    );
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(pid)
        .expect("self message should be available")
    else {
        panic!("self send must deliver one message");
    };
    assert_eq!(message.sender, pid);
    assert_eq!(message.payload, ReplValue::String("wake".to_string()));
}

#[test]
fn actor_context_self_send_isolates_process_mailboxes() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(source("first"));
    let second = runtime.spawn_root(source("second"));
    let first_context = runtime.context(first).expect("first context");
    let second_context = runtime.context(second).expect("second context");

    runtime
        .send_self(first_context, ReplValue::Int(1))
        .expect("first self send");
    runtime
        .send_self(second_context, ReplValue::Int(2))
        .expect("second self send");

    let VmActorReceive::Message(first_message) =
        runtime.receive_next_or_block(first).expect("first receive")
    else {
        panic!("first actor should receive its own message");
    };
    let VmActorReceive::Message(second_message) = runtime
        .receive_next_or_block(second)
        .expect("second receive")
    else {
        panic!("second actor should receive its own message");
    };
    assert_eq!(
        (first_message.sender, first_message.payload),
        (first, ReplValue::Int(1))
    );
    assert_eq!(
        (second_message.sender, second_message.payload),
        (second, ReplValue::Int(2))
    );
}

#[test]
fn actor_context_rejects_missing_exited_and_stale_identities_without_delivery() {
    let mut runtime = VmActorRuntime::default();
    let exited = runtime.spawn_root(source("exited"));
    let recipient = runtime.spawn_root(source("recipient"));
    let stale_context = runtime.context(exited).expect("context should exist");
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("actor should exit");
    let missing = VmProcessId::from_raw_for_test(99);

    assert_eq!(
        runtime
            .context(missing)
            .expect_err("missing context should fail"),
        "cannot create context for missing process 99"
    );
    assert_eq!(
        runtime
            .context(exited)
            .expect_err("exited context should fail"),
        "cannot create context for exited process 1"
    );
    assert_eq!(
        runtime
            .send_self(stale_context, ReplValue::Unit)
            .expect_err("stale self send should fail"),
        "sender process 1 has exited"
    );
    assert_eq!(
        runtime
            .send(exited, recipient, ReplValue::Unit)
            .expect_err("exited sender should fail"),
        "sender process 1 has exited"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient should exist")
            .mailbox_len(),
        0
    );
}
