use super::{VmCoordinationEnvelope, VmCoordinationProfile, VmMessageIdAllocator};
use crate::runtime::vm::term_format::{
    encode_tetf_distribution_envelope, TetfVmRef, TetfVmRefKind,
};
use crate::runtime::vm::ReplValue;

#[test]
fn vm_coordination_accepts_compatible_peer_with_required_capability() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut ids = VmMessageIdAllocator::default();

    let envelope =
        VmCoordinationEnvelope::new(ids.next(), &from, &to, "vm.message.send").expect("envelope");

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
fn vm_coordination_rejects_cross_cluster_peer() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-other", ["vm.message.send"]);

    let error = VmCoordinationEnvelope::new(1, &from, &to, "vm.message.send")
        .expect_err("cross-cluster coordination should fail");

    assert!(error.starts_with("error[vm_coordination]:"));
    assert!(error.contains("incompatible"));
}

#[test]
fn vm_coordination_rejects_missing_capability() {
    let from = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let to = profile("search", "vm-b", "cluster-main", ["vm.inspect"]);

    let error = VmCoordinationEnvelope::new(1, &from, &to, "vm.message.send")
        .expect_err("missing capability should fail");

    assert!(error.starts_with("error[vm_coordination]:"));
    assert!(error.contains("vm.message.send"));
}

#[test]
fn vm_coordination_message_ids_are_monotonic() {
    let mut ids = VmMessageIdAllocator::default();

    assert_eq!(ids.next(), 1);
    assert_eq!(ids.next(), 2);
    assert_eq!(ids.next(), 3);
}

#[test]
fn vm_coordination_builds_tetf_distribution_envelope_with_refs() {
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

fn profile<const N: usize>(
    app_id: &str,
    vm_id: &str,
    cluster_id: &str,
    capabilities: [&str; N],
) -> VmCoordinationProfile {
    VmCoordinationProfile::new(
        app_id,
        vm_id,
        format!("{vm_id}-node"),
        cluster_id,
        7,
        "0.0.7",
        capabilities,
    )
}
