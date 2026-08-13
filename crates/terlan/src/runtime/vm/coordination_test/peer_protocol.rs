use super::*;

use super::super::{
    VmClusterMembership, VmClusterNodeState, VmCoordinationEnvelope, VmDistributedDisconnectReason,
    VmDistributedInboundOutcome, VmDistributedReconnectOutcome, VmDistributedSessionState,
    VmDistributedTransportFrame, VmDistributedTransportSession, VmDistributionDelivery,
    VmMessageIdAllocator,
};
use crate::runtime::vm::term_format::{
    encode_tetf_distribution_envelope, TetfVmRef, TetfVmRefKind,
};
use crate::runtime::vm::ReplValue;

#[test]
pub(super) fn vm_coordination_accepts_compatible_peer_with_required_capability() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut ids = VmMessageIdAllocator::default();

    let message_id = ids.reserve().expect("reserved message id");
    let envelope =
        VmCoordinationEnvelope::new(message_id, &from, &to, "vm.message.send").expect("envelope");
    ids.commit(message_id);

    assert_eq!(envelope.message_id, 1);
    assert_eq!(envelope.from_app_id, "docs");
    assert_eq!(envelope.from_node_id, "vm-a-node");
    assert_eq!(envelope.to_app_id, "search");
    assert_eq!(envelope.to_vm_id, "vm-b");
    assert_eq!(envelope.to_node_id, "vm-b-node");
    assert_eq!(envelope.epoch, 7);
    assert_eq!(envelope.trace_id, "trace:vm-a:vm-b:1");
}

#[test]
pub(super) fn vm_coordination_rejects_cross_cluster_peer() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-other", ["vm.message.send"]);

    let error = VmCoordinationEnvelope::new(1, &from, &to, "vm.message.send")
        .expect_err("cross-cluster coordination should fail");

    assert!(error.starts_with("error[vm_coordination]:"));
    assert!(error.contains("incompatible"));
}

#[test]
pub(super) fn vm_coordination_rejects_missing_capability() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.inspect"]);

    let error = VmCoordinationEnvelope::new(1, &from, &to, "vm.message.send")
        .expect_err("missing capability should fail");

    assert!(error.starts_with("error[vm_coordination]:"));
    assert!(error.contains("vm.message.send"));
}

#[test]
pub(super) fn vm_coordination_message_ids_are_monotonic() {
    let mut ids = VmMessageIdAllocator::default();

    for expected in 1..=3 {
        let reserved = ids.reserve().expect("reserved message id");
        assert_eq!(reserved, expected);
        ids.commit(reserved);
    }
}

#[test]
pub(super) fn vm_coordination_builds_tetf_distribution_envelope_with_refs() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let envelope = VmCoordinationEnvelope::new(3, &from, &to, "vm.message.send")
        .expect("coordination envelope should build");

    let tetf_envelope = envelope.to_tetf_distribution_envelope(
        vec![TetfVmRef::new(TetfVmRefKind::Process, "vm-b-node", 99, 7)],
        ReplValue::String("hello".to_string()),
    );

    assert_eq!(tetf_envelope.trace_id, "trace:vm-a:vm-b:3");
    assert_eq!(tetf_envelope.from_node_id, "vm-a-node");
    assert_eq!(tetf_envelope.to_node_id, "vm-b-node");
    assert_eq!(tetf_envelope.epoch, 7);
    let encoded =
        encode_tetf_distribution_envelope(&tetf_envelope, &[]).expect("TETF should encode");
    assert_eq!(&encoded[0..4], b"TETF");
}

#[test]
pub(super) fn vm_distributed_transport_encodes_bounded_message_frame() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to, 512).expect("session should open");

    let frame = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("hello".to_string()),
            vec![TetfVmRef::new(TetfVmRefKind::Process, "vm-b-node", 99, 7)],
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("frame should encode");

    assert_eq!(frame.message_id, 1);
    assert_eq!(frame.trace_id, "trace:vm-a:vm-b:1");
    assert_eq!(frame.from_node_id, "vm-a-node");
    assert_eq!(frame.to_node_id, "vm-b-node");
    assert_eq!(frame.delivery, VmDistributionDelivery::AtMostOnce);
    assert_eq!(&frame.bytes[0..4], b"TETF");
    assert_eq!(session.pending_ack_count(), 0);
}

#[test]
pub(super) fn vm_distributed_transport_tracks_needs_ack_lifecycle() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to, 512).expect("session should open");

    let frame = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("hello".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect("frame should encode");

    assert!(session.needs_ack(frame.message_id));
    assert_eq!(session.pending_ack_count(), 1);
    session
        .acknowledge(frame.message_id)
        .expect("ack should clear pending message");
    assert!(!session.needs_ack(frame.message_id));
    assert_eq!(session.pending_ack_count(), 0);
    let error = session
        .acknowledge(frame.message_id)
        .expect_err("duplicate ack should fail");
    assert_eq!(
        error,
        "error[vm_distributed_transport]: no pending acknowledgement for message `1`"
    );
}

#[test]
pub(super) fn vm_distributed_transport_rejects_incompatible_profiles_and_zero_limits() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let cross_cluster = profile("search", "vm-b", "cluster-other", ["vm.message.send"]);
    let same_cluster = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);

    let incompatible_error = VmDistributedTransportSession::open(from.clone(), cross_cluster, 512)
        .expect_err("cross-cluster session should fail");
    assert_eq!(
        incompatible_error,
        "error[vm_distributed_transport]: incompatible VM coordination profiles"
    );
    let zero_limit_error = VmDistributedTransportSession::open(from, same_cluster, 0)
        .expect_err("zero message limit should fail");
    assert_eq!(
        zero_limit_error,
        "error[vm_distributed_transport]: max message bytes must be non-zero"
    );
}

#[test]
pub(super) fn vm_distributed_transport_rejects_oversized_encoded_messages_without_pending_ack() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to, 8).expect("session should open");

    let error = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("message larger than limit".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect_err("oversized message should fail");

    assert!(error.starts_with("error[vm_distributed_transport]:"));
    assert_eq!(session.pending_ack_count(), 0);
}

#[test]
pub(super) fn vm_distributed_transport_accepts_inbound_frames_and_rejects_duplicates() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let mut inbound = VmDistributedTransportSession::open(to, from, 512).expect("inbound");
    let frame = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("hello".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("frame");

    let first = inbound
        .accept_inbound_frame(&frame, &[])
        .expect("first frame should classify");
    let second = inbound
        .accept_inbound_frame(&frame, &[])
        .expect("duplicate frame should classify");

    assert_eq!(first, VmDistributedInboundOutcome::Accepted);
    assert_eq!(second, VmDistributedInboundOutcome::Duplicate);
    assert_eq!(inbound.next_inbound_message_id(), 2);
}

#[test]
pub(super) fn vm_distributed_transport_decodes_declared_atom_payload_before_acceptance() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let mut inbound = VmDistributedTransportSession::open(to, from, 512).expect("inbound");
    let declared_atoms = vec![String::from("ready")];
    let frame = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::Atom("ready".to_string()),
            Vec::new(),
            &declared_atoms,
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("frame");

    assert_eq!(
        inbound
            .accept_inbound_frame(&frame, &declared_atoms)
            .expect("declared atom frame should decode and classify"),
        VmDistributedInboundOutcome::Accepted
    );
}

#[test]
pub(super) fn vm_distributed_transport_rejects_corrupt_or_mismatched_tetf_without_advancing() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let mut inbound =
        VmDistributedTransportSession::open(to.clone(), from.clone(), 512).expect("inbound");
    let frame = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("hello".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("frame");

    let mut truncated = frame.clone();
    truncated.bytes.pop();
    let truncated_error = inbound
        .accept_inbound_frame(&truncated, &[])
        .expect_err("truncated TETF should fail");
    assert!(truncated_error.starts_with("error[tetf_truncated]:"));
    assert_eq!(inbound.next_inbound_message_id(), 1);

    let mut mismatched = frame.clone();
    mismatched.trace_id = "trace:vm-a:vm-b:99".to_string();
    assert_eq!(
        inbound
            .accept_inbound_frame(&mismatched, &[])
            .expect_err("frame and envelope metadata mismatch should fail"),
        "error[vm_distributed_transport]: frame `trace:vm-a:vm-b:99` metadata does not match its TETF envelope or session"
    );
    assert_eq!(inbound.next_inbound_message_id(), 1);

    let mut constrained =
        VmDistributedTransportSession::open(to, from, 8).expect("constrained inbound");
    assert_eq!(
        constrained
            .accept_inbound_frame(&frame, &[])
            .expect_err("oversized inbound frame should fail"),
        "error[vm_distributed_transport]: inbound frame `trace:vm-a:vm-b:1` exceeds max message bytes"
    );
    assert_eq!(constrained.next_inbound_message_id(), 1);

    assert_eq!(
        inbound
            .accept_inbound_frame(&frame, &[])
            .expect("valid frame should remain acceptable"),
        VmDistributedInboundOutcome::Accepted
    );
}

#[test]
pub(super) fn vm_distributed_transport_snapshot_preserves_message_and_delivery_continuity() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let first = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("one".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect("first frame");
    let mut restored_outbound =
        VmDistributedTransportSession::restore(from.clone(), to.clone(), 512, outbound.snapshot())
            .expect("outbound snapshot should restore");
    let second = restored_outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("two".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("second frame");

    assert_eq!(first.message_id, 1);
    assert_eq!(second.message_id, 2);
    assert!(restored_outbound.needs_ack(first.message_id));

    let mut inbound =
        VmDistributedTransportSession::open(to.clone(), from.clone(), 512).expect("inbound");
    assert_eq!(
        inbound
            .accept_inbound_frame(&first, &[])
            .expect("first accept"),
        VmDistributedInboundOutcome::Accepted
    );
    let mut restored_inbound =
        VmDistributedTransportSession::restore(to, from, 512, inbound.snapshot())
            .expect("inbound snapshot should restore");
    assert_eq!(
        restored_inbound
            .accept_inbound_frame(&first, &[])
            .expect("duplicate accept"),
        VmDistributedInboundOutcome::Duplicate
    );
    assert_eq!(
        restored_inbound
            .accept_inbound_frame(&second, &[])
            .expect("second accept"),
        VmDistributedInboundOutcome::Accepted
    );
}

#[test]
pub(super) fn vm_distributed_transport_restore_rejects_noncontiguous_inbound_history() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let session = VmDistributedTransportSession::open(from.clone(), to.clone(), 512)
        .expect("session should open");
    let mut snapshot = session.snapshot();
    snapshot.next_inbound_message_id = 3;
    snapshot.accepted_inbound_message_ids = vec![2, 1];

    let error = VmDistributedTransportSession::restore(from, to, 512, snapshot)
        .expect_err("noncontiguous history should fail");

    assert_eq!(
        error,
        "error[vm_distributed_transport]: accepted inbound message ids must form one contiguous prefix"
    );
}

#[test]
pub(super) fn vm_distributed_transport_reports_out_of_order_then_accepts_gap_after_prior_frame() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let mut inbound = VmDistributedTransportSession::open(to, from, 512).expect("inbound");
    let first = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("one".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("first frame");
    let second = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("two".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("second frame");

    let gap = inbound
        .accept_inbound_frame(&second, &[])
        .expect("out-of-order frame should classify");
    let accepted_first = inbound
        .accept_inbound_frame(&first, &[])
        .expect("first frame should classify");
    let accepted_second = inbound
        .accept_inbound_frame(&second, &[])
        .expect("second frame should classify after gap closes");

    assert_eq!(
        gap,
        VmDistributedInboundOutcome::OutOfOrder {
            expected_message_id: 1
        }
    );
    assert_eq!(accepted_first, VmDistributedInboundOutcome::Accepted);
    assert_eq!(accepted_second, VmDistributedInboundOutcome::Accepted);
    assert_eq!(inbound.next_inbound_message_id(), 3);
}

#[test]
pub(super) fn vm_distributed_transport_rejects_frame_for_wrong_session() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let other = profile("other", "vm-c", "cluster-main", ["vm.message.send"]);
    let mut inbound = VmDistributedTransportSession::open(to, from, 512).expect("inbound");
    let frame = VmDistributedTransportFrame {
        message_id: 1,
        trace_id: "trace:vm-c:vm-b:1".to_string(),
        from_node_id: other.node_id().to_string(),
        to_node_id: "vm-b-node".to_string(),
        delivery: VmDistributionDelivery::AtMostOnce,
        bytes: b"TETF".to_vec(),
    };

    let error = inbound
        .accept_inbound_frame(&frame, &[])
        .expect_err("wrong sender should fail");

    assert_eq!(
        error,
        "error[vm_distributed_transport]: frame `trace:vm-c:vm-b:1` is not addressed to this session"
    );
}

#[test]
pub(super) fn vm_distributed_transport_disconnect_blocks_encode_and_accept_until_reconnect() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut outbound =
        VmDistributedTransportSession::open(from.clone(), to.clone(), 512).expect("outbound");
    let mut inbound = VmDistributedTransportSession::open(to.clone(), from.clone(), 512)
        .expect("inbound should open");
    let frame = outbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("hello".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("frame should encode");

    let disconnect = inbound.disconnect(VmDistributedDisconnectReason::TransportFailure, 42);
    assert_eq!(
        disconnect.reason,
        VmDistributedDisconnectReason::TransportFailure
    );
    assert_eq!(disconnect.tick, 42);
    assert_eq!(disconnect.pending_ack_count, 0);
    assert_eq!(inbound.state(), VmDistributedSessionState::Disconnected);
    assert_eq!(
        inbound.last_disconnect().expect("disconnect should record"),
        &disconnect
    );

    let accept_error = inbound
        .accept_inbound_frame(&frame, &[])
        .expect_err("disconnected accept should fail");
    assert_eq!(
        accept_error,
        "error[vm_distributed_transport]: session is disconnected; reconnect before message operations"
    );
    let encode_error = inbound
        .encode_message(
            "vm.message.send",
            ReplValue::String("blocked".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect_err("disconnected encode should fail");
    assert_eq!(encode_error, accept_error);

    let outcome = inbound.reconnect(&from, 43).expect("reconnect should work");
    assert_eq!(
        outcome,
        VmDistributedReconnectOutcome::Reconnected {
            pending_ack_count: 0
        }
    );
    assert_eq!(inbound.state(), VmDistributedSessionState::Connected);
    assert_eq!(inbound.last_reconnect_tick(), Some(43));
    assert_eq!(
        inbound
            .accept_inbound_frame(&frame, &[])
            .expect("after reconnect"),
        VmDistributedInboundOutcome::Accepted
    );
}

#[test]
pub(super) fn vm_distributed_transport_reconnect_preserves_pending_acks_and_rejects_wrong_remote() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let other = profile("other", "vm-c", "cluster-main", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to.clone(), 512).expect("session should open");
    let frame = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("needs ack".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect("frame should encode");

    let already_connected = session
        .reconnect(&to, 5)
        .expect("connected reconnect should classify");
    assert_eq!(
        already_connected,
        VmDistributedReconnectOutcome::AlreadyConnected
    );

    session.disconnect(VmDistributedDisconnectReason::HeartbeatTimeout, 10);
    let wrong_remote_error = session
        .reconnect(&other, 11)
        .expect_err("wrong remote should fail");
    assert_eq!(
        wrong_remote_error,
        "error[vm_distributed_transport]: reconnect profile `vm-c-node` does not match session remote `vm-b-node`"
    );
    assert_eq!(session.state(), VmDistributedSessionState::Disconnected);

    let outcome = session
        .reconnect(&to, 12)
        .expect("same remote should reconnect");
    assert_eq!(
        outcome,
        VmDistributedReconnectOutcome::Reconnected {
            pending_ack_count: 1
        }
    );
    assert!(session.needs_ack(frame.message_id));
    session
        .acknowledge(frame.message_id)
        .expect("pending ack should survive reconnect");
}

#[test]
pub(super) fn vm_distributed_transport_disconnect_blocks_ack_until_reconnect() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to.clone(), 512).expect("session should open");
    let frame = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("needs ack".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect("frame should encode");

    session.disconnect(VmDistributedDisconnectReason::TransportFailure, 10);
    let disconnected_ack = session
        .acknowledge(frame.message_id)
        .expect_err("disconnected ack should fail");

    assert_eq!(
        disconnected_ack,
        "error[vm_distributed_transport]: session is disconnected; reconnect before message operations"
    );
    assert!(session.needs_ack(frame.message_id));
    assert_eq!(session.pending_ack_count(), 1);
    session
        .reconnect(&to, 11)
        .expect("same remote should reconnect");
    session
        .acknowledge(frame.message_id)
        .expect("ack should clear after reconnect");
    assert_eq!(session.pending_ack_count(), 0);
}

#[test]
pub(super) fn vm_distributed_transport_rejects_incompatible_reconnect_profile() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let incompatible = profile("search", "vm-b", "cluster-other", ["vm.message.send"]);
    let mut session =
        VmDistributedTransportSession::open(from, to, 512).expect("session should open");

    session.disconnect(VmDistributedDisconnectReason::HeartbeatTimeout, 10);
    let error = session
        .reconnect(&incompatible, 11)
        .expect_err("cross-cluster reconnect should fail");

    assert_eq!(
        error,
        "error[vm_distributed_transport]: incompatible VM coordination profile on reconnect"
    );
    assert_eq!(session.state(), VmDistributedSessionState::Disconnected);
}

#[test]
pub(super) fn vm_cluster_membership_joins_peer_with_deterministic_role_tags() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 10).expect("membership should create");

    membership
        .join_peer(&peer, 3, ["worker", "search", "worker"])
        .expect("peer should join");

    let view = membership.view();
    assert_eq!(view.len(), 2);
    assert_eq!(view[0].node_id, "vm-a-node");
    assert_eq!(view[1].node_id, "vm-b-node");
    assert_eq!(view[1].state, VmClusterNodeState::Active);
    assert_eq!(view[1].last_seen_tick, 3);
    assert_eq!(view[1].role_tags, vec!["search", "worker"]);
}

#[test]
pub(super) fn vm_cluster_membership_rejects_incompatible_peer_and_zero_timeout() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-other", ["vm.message.send"]);
    let mut membership =
        VmClusterMembership::new(local.clone(), 10).expect("membership should create");

    let join_error = membership
        .join_peer(&peer, 1, ["worker"])
        .expect_err("cross-cluster peer should fail");
    assert_eq!(
        join_error,
        "error[vm_cluster_membership]: incompatible VM coordination profile"
    );
    let timeout_error =
        VmClusterMembership::new(local, 0).expect_err("zero heartbeat timeout should fail");
    assert_eq!(
        timeout_error,
        "error[vm_cluster_membership]: heartbeat timeout ticks must be non-zero"
    );
}

#[test]
pub(super) fn vm_cluster_membership_expires_and_recovers_unreachable_peer_by_heartbeat() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should create");
    membership
        .join_peer(&peer, 2, ["worker"])
        .expect("peer should join");

    let expired = membership.expire_stale_nodes(8);

    assert_eq!(expired, vec!["vm-b-node"]);
    assert_eq!(
        membership.node("vm-a-node").expect("local").state,
        VmClusterNodeState::Active
    );
    assert_eq!(
        membership.node("vm-b-node").expect("peer").state,
        VmClusterNodeState::Unreachable
    );
    assert_eq!(
        membership
            .record_heartbeat("vm-b-node", 9)
            .expect("heartbeat should recover"),
        VmClusterNodeState::Active
    );
    assert_eq!(
        membership.node("vm-b-node").expect("peer").last_seen_tick,
        9
    );
}

#[test]
pub(super) fn vm_cluster_membership_keeps_fresh_peer_active_before_timeout() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should create");
    membership
        .join_peer(&peer, 2, ["worker"])
        .expect("peer should join");

    assert!(membership.expire_stale_nodes(7).is_empty());
    assert_eq!(
        membership.node("vm-b-node").expect("peer").state,
        VmClusterNodeState::Active
    );
}

#[test]
pub(super) fn vm_cluster_membership_rejects_stale_left_and_fenced_heartbeats() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should create");
    membership
        .join_peer(&peer, 4, ["worker"])
        .expect("peer should join");

    let stale_error = membership
        .record_heartbeat("vm-b-node", 3)
        .expect_err("stale heartbeat should fail");
    assert_eq!(
        stale_error,
        "error[vm_cluster_membership]: stale heartbeat for node `vm-b-node`"
    );
    membership
        .mark_left("vm-b-node", 6)
        .expect("peer should leave");
    let left_error = membership
        .record_heartbeat("vm-b-node", 7)
        .expect_err("left node heartbeat should fail");
    assert_eq!(
        left_error,
        "error[vm_cluster_membership]: node `vm-b-node` is not heartbeat-eligible"
    );
    membership
        .fence_node("vm-b-node", 8)
        .expect("peer should fence");
    let rejoin_error = membership
        .join_peer(&peer, 9, ["worker"])
        .expect_err("fenced peer should not rejoin");
    assert_eq!(
        rejoin_error,
        "error[vm_cluster_membership]: fenced node `vm-b-node` cannot rejoin"
    );
}

#[test]
pub(super) fn vm_cluster_membership_rejects_fenced_leave() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should create");
    membership
        .join_peer(&peer, 4, ["worker"])
        .expect("peer should join");
    membership
        .fence_node("vm-b-node", 8)
        .expect("peer should fence");

    let error = membership
        .mark_left("vm-b-node", 9)
        .expect_err("fenced peer cannot leave");

    assert_eq!(
        error,
        "error[vm_cluster_membership]: fenced node `vm-b-node` cannot leave"
    );
}

#[test]
pub(super) fn vm_cluster_membership_reports_unknown_node_operations() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should create");

    assert_eq!(
        membership
            .record_heartbeat("missing-node", 1)
            .expect_err("missing heartbeat node should fail"),
        "error[vm_cluster_membership]: unknown node `missing-node`"
    );
    assert_eq!(
        membership
            .mark_left("missing-node", 1)
            .expect_err("missing left node should fail"),
        "error[vm_cluster_membership]: unknown node `missing-node`"
    );
    assert_eq!(
        membership
            .fence_node("missing-node", 1)
            .expect_err("missing fence node should fail"),
        "error[vm_cluster_membership]: unknown node `missing-node`"
    );
}

#[test]
pub(super) fn vm_cluster_membership_prunes_only_expired_terminal_snapshots() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let departed = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let fenced = profile("admin", "vm-c", "cluster-main", ["vm.message.send"]);
    let active = profile("web", "vm-d", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should build");
    membership
        .join_peer(&departed, 1, ["worker"])
        .expect("departed peer should join");
    membership
        .join_peer(&fenced, 1, ["worker"])
        .expect("fenced peer should join");
    membership
        .join_peer(&active, 1, ["worker"])
        .expect("active peer should join");
    membership
        .mark_left(departed.node_id(), 2)
        .expect("peer should leave");
    membership
        .fence_node(fenced.node_id(), 2)
        .expect("peer should fence");

    assert_eq!(
        membership
            .prune_stale_nodes(7, 5)
            .expect("exact retention boundary should be valid"),
        Vec::<String>::new()
    );
    assert!(membership.node(departed.node_id()).is_some());
    assert_eq!(
        membership
            .prune_stale_nodes(8, 5)
            .expect("expired terminal peer should prune"),
        vec![departed.node_id().to_string()]
    );
    assert!(membership.node(departed.node_id()).is_none());
    assert_eq!(
        membership.node(fenced.node_id()).map(|node| node.state),
        Some(VmClusterNodeState::Fenced)
    );
    assert_eq!(
        membership.node(active.node_id()).map(|node| node.state),
        Some(VmClusterNodeState::Active)
    );
    assert_eq!(
        membership
            .prune_stale_nodes(100, 5)
            .expect("protected states should remain"),
        Vec::<String>::new()
    );
}
