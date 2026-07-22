use super::*;

fn suite_profile(
    app_id: &str,
    vm_id: &str,
    node_id: &str,
    epoch: u64,
    capabilities: &[&str],
) -> VmCoordinationProfile {
    VmCoordinationProfile::new(
        app_id,
        vm_id,
        node_id,
        "distribution-suite",
        epoch,
        "0.0.7",
        capabilities.iter().copied(),
    )
    .expect("suite profile")
}

#[test]
fn distribution_suite_bulk_delivery_is_ordered_deduplicated_and_transactional() {
    let sender = suite_profile(
        "sender-app",
        "sender-vm",
        "sender-node",
        1,
        &["vm.message.send"],
    );
    let receiver = suite_profile(
        "receiver-app",
        "receiver-vm",
        "receiver-node",
        7,
        &["vm.message.send"],
    );
    let atoms = vec!["bulk".to_string()];
    let mut outbound =
        VmDistributedTransportSession::open(sender.clone(), receiver.clone(), 256 * 1024)
            .expect("outbound session");
    let mut inbound =
        VmDistributedTransportSession::open(receiver.clone(), sender.clone(), 256 * 1024)
            .expect("inbound session");
    let mut frames = Vec::new();

    for index in 0..256_u64 {
        let delivery = if index % 3 == 0 {
            VmDistributionDelivery::NeedsAck
        } else {
            VmDistributionDelivery::AtMostOnce
        };
        let payload = ReplValue::Tuple(vec![
            ReplValue::Atom("bulk".to_string()),
            ReplValue::Int(index as i64),
            ReplValue::Bytes(vec![(index % 251) as u8; 1024].into()),
        ]);
        let frame = outbound
            .encode_message(
                "vm.message.send",
                payload,
                vec![TetfVmRef::new(
                    super::super::term_format::TetfVmRefKind::Process,
                    "receiver-node",
                    index + 1,
                    7,
                )],
                &atoms,
                delivery,
            )
            .expect("bounded bulk frame");
        assert_eq!(frame.message_id, index + 1);
        frames.push(frame);
    }

    assert_eq!(outbound.pending_ack_count(), 86);
    assert_eq!(
        inbound
            .accept_inbound_frame(&frames[1], &atoms)
            .expect("gap classification"),
        VmDistributedInboundOutcome::OutOfOrder {
            expected_message_id: 1
        }
    );
    for frame in &frames {
        assert_eq!(
            inbound
                .accept_inbound_frame(frame, &atoms)
                .expect("ordered bulk acceptance"),
            VmDistributedInboundOutcome::Accepted
        );
        if frame.delivery == VmDistributionDelivery::NeedsAck {
            outbound
                .acknowledge(frame.message_id)
                .expect("exact pending ack");
        }
    }
    assert_eq!(inbound.next_inbound_message_id(), 257);
    assert_eq!(outbound.pending_ack_count(), 0);
    assert_eq!(
        inbound
            .accept_inbound_frame(&frames[255], &atoms)
            .expect("duplicate classification"),
        VmDistributedInboundOutcome::Duplicate
    );

    let mut constrained =
        VmDistributedTransportSession::open(sender, receiver, 512).expect("constrained session");
    let before = constrained.snapshot();
    let error = constrained
        .encode_message(
            "vm.message.send",
            ReplValue::Bytes(vec![0; 4096].into()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect_err("oversized frame must fail");
    assert!(error.contains("exceeds max message bytes"));
    assert_eq!(constrained.snapshot(), before);
    let first = constrained
        .encode_message(
            "vm.message.send",
            ReplValue::Int(1),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .expect("failure must not consume message id");
    assert_eq!(first.message_id, 1);
}

#[test]
fn distribution_suite_node_lifecycle_restart_and_reconnect_are_generation_safe() {
    let local = suite_profile(
        "control-app",
        "control-vm",
        "control-node",
        3,
        &["vm.message.send"],
    );
    let visible = suite_profile(
        "visible-app",
        "visible-vm",
        "visible-node",
        11,
        &["vm.message.send"],
    );
    let hidden = suite_profile(
        "hidden-app",
        "hidden-vm",
        "hidden-node",
        19,
        &["vm.message.send"],
    );
    let mut membership = VmClusterMembership::new(local.clone(), 10).expect("membership view");
    membership
        .join_peer(&visible, 2, ["visible", "worker", "visible"])
        .expect("visible peer");
    membership
        .join_peer(&hidden, 3, ["hidden", "worker"])
        .expect("hidden peer");

    let view = membership.view();
    assert_eq!(
        view.iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["control-node", "hidden-node", "visible-node"]
    );
    assert_eq!(
        membership.node("control-node").unwrap().state,
        VmClusterNodeState::Active
    );
    assert_eq!(membership.node("missing-node"), None);
    assert_eq!(
        membership.node("visible-node").unwrap().role_tags,
        vec!["visible", "worker"]
    );
    assert_eq!(
        membership.node("hidden-node").unwrap().role_tags,
        vec!["hidden", "worker"]
    );

    membership
        .partition_node("visible-node", 5)
        .expect("partition visible peer");
    assert_eq!(
        membership.node("visible-node").unwrap().state,
        VmClusterNodeState::Unreachable
    );
    membership
        .heal_node("visible-node", 6)
        .expect("heal visible peer");
    membership
        .record_heartbeat("hidden-node", 8)
        .expect("hidden peer heartbeat");
    assert_eq!(membership.expire_stale_nodes(17), vec!["visible-node"]);
    membership
        .mark_left("hidden-node", 18)
        .expect("hidden peer leaves");

    let restarted_visible = visible.next_epoch().expect("new visible incarnation");
    membership
        .restart_peer(&restarted_visible, 20)
        .expect("newer incarnation restarts");
    assert_eq!(
        membership.node("visible-node").unwrap().state,
        VmClusterNodeState::Active
    );
    assert!(membership.restart_peer(&visible, 21).is_err());
    assert_eq!(
        membership
            .prune_stale_nodes(40, 10)
            .expect("prune terminal peers"),
        vec!["hidden-node"]
    );

    let mut session = VmDistributedTransportSession::open(local, restarted_visible.clone(), 4096)
        .expect("restarted transport");
    let pending = session
        .encode_message(
            "vm.message.send",
            ReplValue::String("before disconnect".to_string()),
            Vec::new(),
            &[],
            VmDistributionDelivery::NeedsAck,
        )
        .expect("pending frame");
    let event = session.disconnect(VmDistributedDisconnectReason::TransportFailure, 22);
    assert_eq!(event.pending_ack_count, 1);
    assert!(session
        .encode_message(
            "vm.message.send",
            ReplValue::Int(2),
            Vec::new(),
            &[],
            VmDistributionDelivery::AtMostOnce,
        )
        .is_err());
    let wrong_identity = suite_profile(
        "other-app",
        "other-vm",
        "other-node",
        1,
        &["vm.message.send"],
    );
    assert!(session.reconnect(&wrong_identity, 23).is_err());
    assert_eq!(
        session
            .reconnect(&restarted_visible, 24)
            .expect("same incarnation reconnects"),
        VmDistributedReconnectOutcome::Reconnected {
            pending_ack_count: 1
        }
    );
    session
        .acknowledge(pending.message_id)
        .expect("pending ack survives reconnect");
    assert_eq!(session.pending_ack_count(), 0);
}
