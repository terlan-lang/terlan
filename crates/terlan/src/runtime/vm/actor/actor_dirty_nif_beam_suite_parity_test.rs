use std::sync::Arc;

use super::super::super::process::{VmProcessSource, VmProcessState};
use super::super::{ReplValue, VmActorReceive, VmActorRuntime};

fn source(function: impl Into<String>) -> VmProcessSource {
    VmProcessSource::new("parity.DirtyNativeAdapter", function, 0)
}

fn managed_payload(width: usize) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Int(42),
        ReplValue::String("Terlan".to_string()),
        ReplValue::Bytes(Arc::from(&b"native\0"[..])),
        ReplValue::List(
            (0..width)
                .map(|index| {
                    ReplValue::Tuple(vec![ReplValue::Int(index as i64), ReplValue::Bool(true)])
                })
                .collect(),
        ),
    ])
}

#[test]
fn dirty_nif_suite_managed_native_send_preserves_value_and_mailbox_ownership() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("managed_sender"));
    let receiver = runtime.spawn_root(source("managed_receiver"));
    let payload = managed_payload(1_000);

    runtime
        .park_native_continuation(owner.as_u64(), 211, 223)
        .expect("native managed send parks");
    let message_id = runtime
        .service_native_send(owner.as_u64(), 211, 223, receiver.as_u64(), payload.clone())
        .expect("managed native value is delivered");
    assert!(message_id > 0);
    assert_eq!(runtime.pending_native_continuation_count(), 0);
    assert_eq!(
        runtime.processes().get(owner).expect("owner resumes").state,
        VmProcessState::Runnable
    );

    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(receiver)
        .expect("receiver consumes managed value")
    else {
        panic!("managed native send must enqueue one message");
    };
    assert_eq!(message.sender, owner);
    assert_eq!(message.payload, payload);
    assert_eq!(
        runtime
            .memory_metrics(receiver)
            .expect("receiver memory metrics")
            .current_bytes,
        0
    );
}

#[test]
fn dirty_nif_suite_registry_burst_routes_exactly_once_and_cleans_stale_names() {
    const ACTORS: usize = 64;
    let mut runtime = VmActorRuntime::default();
    let actors = (0..ACTORS)
        .map(|index| runtime.spawn_root(source(format!("registry_actor_{index}"))))
        .collect::<Vec<_>>();
    for (index, actor) in actors.iter().copied().enumerate() {
        runtime
            .register_name(format!("dirty.native.{index}"), actor)
            .expect("register native target");
    }

    for (index, owner) in actors.iter().copied().enumerate() {
        let request_id = 1_000 + index as u64;
        let continuation_id = 2_000 + index as u64;
        let target = (index + 1) % ACTORS;
        runtime
            .park_native_continuation(owner.as_u64(), request_id, continuation_id)
            .expect("native registry send parks");
        runtime
            .service_native_named_send(
                owner.as_u64(),
                request_id,
                continuation_id,
                &format!("dirty.native.{target}"),
                ReplValue::Tuple(vec![
                    ReplValue::Atom("forward".to_string()),
                    ReplValue::Int(index as i64),
                ]),
            )
            .expect("registered target receives native message");
    }

    assert_eq!(runtime.pending_native_continuation_count(), 0);
    for (index, receiver) in actors.iter().copied().enumerate() {
        let expected_sender = (index + ACTORS - 1) % ACTORS;
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(receiver)
            .expect("receive registry-routed message")
        else {
            panic!("registered native target must receive exactly one message");
        };
        assert_eq!(message.sender, actors[expected_sender]);
        assert_eq!(
            message.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("forward".to_string()),
                ReplValue::Int(expected_sender as i64),
            ])
        );
        assert_eq!(
            runtime
                .processes()
                .get(receiver)
                .expect("receiver remains live")
                .mailbox_len(),
            0
        );
    }

    let retired = actors[0];
    runtime
        .exit_actor(retired, super::super::super::process::VmExitReason::Normal)
        .expect("registered target exits");
    assert_eq!(runtime.lookup_name("dirty.native.0"), None);

    let owner = actors[1];
    runtime
        .park_native_continuation(owner.as_u64(), 3_001, 4_001)
        .expect("missing-name attempt parks");
    assert_eq!(
        runtime
            .service_native_named_send(
                owner.as_u64(),
                3_001,
                4_001,
                "dirty.native.0",
                ReplValue::Unit,
            )
            .expect_err("stale native name must fail without delivery"),
        "actor name `dirty.native.0` is not registered"
    );
    assert_eq!(runtime.pending_native_continuation_count(), 1);
    assert!(matches!(
        runtime
            .processes()
            .get(owner)
            .expect("owner remains parked")
            .state,
        VmProcessState::Suspended(_)
    ));
    runtime
        .exit_actor(owner, super::super::super::process::VmExitReason::Killed)
        .expect("owner exit releases failed native lease");
    assert_eq!(runtime.pending_native_continuation_count(), 0);
}
