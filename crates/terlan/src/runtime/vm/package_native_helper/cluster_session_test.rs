//! Executable transport boundaries, independent of source lowering tests.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn call(
    runtime: &mut VmClusterRuntime,
    operation: &str,
    args: &[ReplValue],
    atoms: &[String],
) -> VmRuntimeResult<ReplValue> {
    runtime.call_with_atoms(
        17,
        &PureNativeCapabilityRequest {
            capability: "package-native".into(),
            operation: format!("std.vm.cluster.{operation}"),
            arguments: Vec::new(),
            package_arguments: Some(args.to_vec()),
            result_type: TvmBoundaryType::Int,
        },
        atoms,
    )
}

fn string(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}
fn atom(value: &str) -> ReplValue {
    ReplValue::Atom(value.into())
}

fn field(value: &ReplValue, key: &str) -> ReplValue {
    let ReplValue::Record { fields, .. } = value else {
        panic!("expected result record")
    };
    fields
        .iter()
        .find(|(name, _)| name == key)
        .expect("declared field")
        .1
        .clone()
}

fn profiles(runtime: &mut VmClusterRuntime) -> (ReplValue, ReplValue) {
    let mut profile = |vm: &str, node: &str| {
        call(
            runtime,
            "profile",
            &[
                string("app"),
                string(vm),
                string(node),
                string("cluster"),
                string("1"),
                string("message"),
            ],
            &[],
        )
        .unwrap()
    };
    (profile("a", "node-a"), profile("b", "node-b"))
}

fn open(runtime: &mut VmClusterRuntime, local: &ReplValue, remote: &ReplValue) -> ReplValue {
    call(
        runtime,
        "open",
        &[local.clone(), remote.clone(), ReplValue::Int(1024)],
        &[],
    )
    .unwrap()
}

fn send(runtime: &mut VmClusterRuntime, session: &ReplValue, payload: ReplValue) -> ReplValue {
    call(
        runtime,
        "send_with",
        &[
            session.clone(),
            string("message"),
            payload,
            atom("needs_ack"),
        ],
        &[],
    )
    .unwrap()
}

#[test]
fn acknowledgements_bind_exact_frames_across_independent_and_forked_sessions() {
    let mut runtime = VmClusterRuntime::default();
    let (local, remote) = profiles(&mut runtime);
    let original = open(&mut runtime, &local, &remote);
    let other = open(&mut runtime, &local, &remote);
    let first = send(&mut runtime, &original, string("one"));
    let fork = send(&mut runtime, &original, string("fork"));
    let unrelated = send(&mut runtime, &other, string("other"));
    let advanced = field(&first, "session");
    for invalid in [field(&fork, "frame"), field(&unrelated, "frame")] {
        for operation in ["acknowledge", "needs_ack"] {
            assert!(call(
                &mut runtime,
                operation,
                &[advanced.clone(), invalid.clone()],
                &[]
            )
            .unwrap_err()
            .to_string()
            .contains("vm.cluster.frame_owner"));
        }
    }
    let frame = field(&first, "frame");
    let acknowledged = call(
        &mut runtime,
        "acknowledge",
        &[advanced.clone(), frame.clone()],
        &[],
    )
    .unwrap();
    let after = field(&acknowledged, "session");
    assert_eq!(
        call(&mut runtime, "pending_ack_count", &[original], &[]).unwrap(),
        ReplValue::Int(0)
    );
    assert_eq!(
        call(&mut runtime, "needs_ack", &[advanced, frame.clone()], &[]).unwrap(),
        ReplValue::Bool(true)
    );
    assert_eq!(
        call(
            &mut runtime,
            "needs_ack",
            &[after.clone(), frame.clone()],
            &[]
        )
        .unwrap(),
        ReplValue::Bool(false)
    );
    assert!(call(&mut runtime, "acknowledge", &[after, frame], &[]).is_err());
}

#[test]
fn receiver_atom_manifest_is_not_inferred_from_sender_payload() {
    let mut runtime = VmClusterRuntime::default();
    let (local, remote) = profiles(&mut runtime);
    let outbound = open(&mut runtime, &local, &remote);
    let inbound = open(&mut runtime, &remote, &local);
    let args = [outbound.clone(), string("message"), atom("known")];
    assert!(call(&mut runtime, "send", &args, &[]).is_err());
    let atoms = ["known".to_string()];
    let sent = call(&mut runtime, "send", &args, &atoms).unwrap();
    let frame = field(&sent, "frame");
    assert_eq!(
        call(&mut runtime, "frame_message_id", &[frame.clone()], &[]).unwrap(),
        ReplValue::Int(1)
    );
    assert!(call(
        &mut runtime,
        "accept",
        &[inbound.clone(), frame.clone()],
        &[]
    )
    .is_err());
    let accepted = call(&mut runtime, "accept", &[inbound, frame.clone()], &atoms).unwrap();
    assert_eq!(field(&accepted, "outcome"), atom("accepted"));
    let duplicate = call(
        &mut runtime,
        "accept",
        &[field(&accepted, "session"), frame],
        &atoms,
    )
    .unwrap();
    assert_eq!(field(&duplicate, "outcome"), atom("duplicate"));
}

#[test]
fn failed_encoding_preserves_message_identity_and_enforces_capability_and_size() {
    let mut runtime = VmClusterRuntime::default();
    let (local, remote) = profiles(&mut runtime);
    for maximum in [0, -1] {
        assert!(call(
            &mut runtime,
            "open",
            &[local.clone(), remote.clone(), ReplValue::Int(maximum)],
            &[]
        )
        .is_err());
    }
    let session = open(&mut runtime, &local, &remote);
    for args in [
        vec![session.clone(), string("forbidden"), string("payload")],
        vec![
            session.clone(),
            string("message"),
            string(&"x".repeat(2048)),
        ],
    ] {
        assert!(call(&mut runtime, "send", &args, &[]).is_err());
    }
    let sent = send(&mut runtime, &session, string("bounded"));
    assert_eq!(
        call(
            &mut runtime,
            "frame_message_id",
            &[field(&sent, "frame")],
            &[]
        )
        .unwrap(),
        ReplValue::Int(1)
    );
}

#[test]
fn disconnect_reasons_are_lossless_and_owner_cleanup_revokes_session_and_frame() {
    let mut runtime = VmClusterRuntime::default();
    let (local, remote) = profiles(&mut runtime);
    let original = open(&mut runtime, &local, &remote);
    let sent = send(&mut runtime, &original, string("pending"));
    let session = field(&sent, "session");
    let frame = field(&sent, "frame");
    for reason in [
        "local_close",
        "remote_close",
        "transport_failure",
        "heartbeat_timeout",
        "session_fenced",
    ] {
        let result = call(
            &mut runtime,
            "disconnect",
            &[session.clone(), atom(reason), ReplValue::Int(10)],
            &[],
        )
        .unwrap();
        let event = field(&result, "event");
        assert_eq!(field(&event, "reason"), atom(reason));
        assert_eq!(field(&event, "pending_ack_count"), ReplValue::Int(1));
        let disconnected = field(&result, "session");
        assert!(call(
            &mut runtime,
            "acknowledge",
            &[disconnected.clone(), frame.clone()],
            &[]
        )
        .is_err());
        assert!(call(
            &mut runtime,
            "send",
            &[disconnected.clone(), string("message"), string("blocked")],
            &[]
        )
        .is_err());
        assert!(call(
            &mut runtime,
            "reconnect",
            &[disconnected.clone(), remote.clone(), ReplValue::Int(9)],
            &[]
        )
        .is_err());
        let resumed = call(
            &mut runtime,
            "reconnect",
            &[disconnected, remote.clone(), ReplValue::Int(11)],
            &[],
        )
        .unwrap();
        assert_eq!(
            field(&field(&resumed, "outcome"), "pending_ack_count"),
            ReplValue::Int(1)
        );
    }
    runtime.close_owner(17);
    assert!(call(&mut runtime, "session_state", &[session], &[]).is_err());
    assert!(call(&mut runtime, "frame_message_id", &[frame], &[]).is_err());
}

#[test]
fn reconnect_checks_application_identity_and_epoch_even_when_already_connected() {
    let mut runtime = VmClusterRuntime::default();
    let (local, remote) = profiles(&mut runtime);
    let advanced = call(&mut runtime, "profile_next_epoch", &[remote.clone()], &[]).unwrap();
    let session = open(&mut runtime, &local, &advanced);
    let impostor = call(
        &mut runtime,
        "profile",
        &[
            string("different-app"),
            string("b"),
            string("node-b"),
            string("cluster"),
            string("1"),
            string("message"),
        ],
        &[],
    )
    .unwrap();
    for invalid in [impostor, remote] {
        assert!(call(
            &mut runtime,
            "reconnect",
            &[session.clone(), invalid, ReplValue::Int(1)],
            &[]
        )
        .is_err());
    }
}
