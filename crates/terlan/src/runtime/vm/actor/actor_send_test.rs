use super::super::{VmActorReceive, VmActorRuntime};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource},
    ReplValue,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Send", name, 0)
}

fn structured_payload() -> ReplValue {
    ReplValue::Record {
        name: "Envelope".to_string(),
        fields: vec![
            ("sequence".to_string(), ReplValue::Int(7)),
            (
                "body".to_string(),
                ReplValue::Tuple(vec![
                    ReplValue::Atom("ready".to_string()),
                    ReplValue::List(vec![ReplValue::Bool(true), ReplValue::Int(42)]),
                ]),
            ),
        ],
    }
}

fn expect_message(receive: VmActorReceive) -> ReplValue {
    match receive {
        VmActorReceive::Message(message) => message.payload,
        other => panic!("expected actor message, got {other:?}"),
    }
}

#[test]
fn actor_send_preserves_structured_payloads_across_pid_name_and_self_routes() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let context = runtime.context(sender).expect("sender context");
    runtime
        .register_name("recipient", recipient)
        .expect("recipient name");
    let payload = structured_payload();

    assert_eq!(
        runtime
            .send(sender, recipient, payload.clone())
            .expect("PID send"),
        1
    );
    assert_eq!(
        runtime
            .send_named(sender, "recipient", payload.clone())
            .expect("named send"),
        2
    );
    assert_eq!(
        runtime
            .send_self(context, payload.clone())
            .expect("self send"),
        3
    );

    assert_eq!(
        expect_message(
            runtime
                .receive_next_or_block(recipient)
                .expect("first recipient message"),
        ),
        payload
    );
    assert_eq!(
        expect_message(
            runtime
                .receive_next_or_block(recipient)
                .expect("second recipient message"),
        ),
        structured_payload()
    );
    assert_eq!(
        expect_message(
            runtime
                .receive_next_or_block(sender)
                .expect("sender self-message"),
        ),
        structured_payload()
    );
    assert_eq!(
        runtime
            .memory_metrics(recipient)
            .expect("recipient memory metrics")
            .current_bytes,
        0
    );
}

#[test]
fn actor_send_validates_sender_before_named_or_alias_destination_resolution() {
    let mut runtime = VmActorRuntime::default();
    let exited_sender = runtime.spawn_root(source("exited-sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    let alias = runtime.create_alias(recipient).expect("recipient alias");
    runtime.remove_alias(alias).expect("remove recipient alias");
    runtime
        .exit_actor(exited_sender, VmExitReason::Normal)
        .expect("sender exit");
    let missing_sender = VmProcessId::from_raw_for_test(404);

    assert_eq!(
        runtime
            .send_named(missing_sender, "missing", ReplValue::Unit)
            .expect_err("missing sender must fail first"),
        "missing sender process 404"
    );
    assert_eq!(
        runtime
            .send_named(exited_sender, "missing", ReplValue::Unit)
            .expect_err("exited sender must fail first"),
        "sender process 1 has exited"
    );
    assert_eq!(
        runtime
            .send_alias(exited_sender, alias, ReplValue::Unit)
            .expect_err("exited sender must hide stale alias state"),
        "sender process 1 has exited"
    );
    assert_eq!(
        runtime
            .processes()
            .get(recipient)
            .expect("recipient")
            .mailbox_len(),
        0
    );
    assert!(runtime.memory_metrics(recipient).is_none());
}

/// Replaces OTP's `dummy_via` helper with one VM-owned custom-name lifecycle.
#[test]
fn actor_custom_name_registry_preserves_via_registration_routing_and_cleanup_semantics() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let owner = runtime.spawn_root(source("owner"));
    let contender = runtime.spawn_root(source("contender"));
    let payload = ReplValue::Tuple(vec![ReplValue::Atom("via".to_string()), ReplValue::Int(7)]);

    runtime
        .register_name("via.worker", owner)
        .expect("initial custom name should register");
    assert_eq!(runtime.lookup_name("via.worker"), Some(owner));
    assert_eq!(
        runtime
            .register_name("via.worker", contender)
            .expect_err("a second live owner must not steal the name"),
        "actor name `via.worker` is already registered to process 2"
    );
    assert_eq!(runtime.lookup_name("via.worker"), Some(owner));

    assert_eq!(
        runtime
            .send_named(sender, "via.worker", payload.clone())
            .expect("named send should route to the registered owner"),
        1
    );
    assert_eq!(
        expect_message(
            runtime
                .receive_next_or_block(owner)
                .expect("registered owner should receive the named message"),
        ),
        payload
    );

    assert_eq!(
        runtime
            .unregister_name("via.worker")
            .expect("custom name should unregister"),
        owner
    );
    assert_eq!(runtime.lookup_name("via.worker"), None);
    assert_eq!(
        runtime
            .send_named(sender, "via.worker", ReplValue::Unit)
            .expect_err("sending through an unregistered name must fail"),
        "actor name `via.worker` is not registered"
    );

    runtime
        .register_name("via.worker", contender)
        .expect("unregistered name should be reusable");
    runtime
        .exit_actor(contender, VmExitReason::Killed)
        .expect("registered owner should exit");
    assert_eq!(runtime.lookup_name("via.worker"), None);
    assert_eq!(
        runtime
            .unregister_name("via.worker")
            .expect_err("owner exit should remove the registration"),
        "actor name `via.worker` is not registered"
    );

    runtime
        .register_name("via.worker", owner)
        .expect("name cleaned on owner exit should be reusable");
    assert_eq!(runtime.lookup_name("via.worker"), Some(owner));
}
