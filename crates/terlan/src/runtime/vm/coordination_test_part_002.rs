
#[test]
fn vm_cluster_membership_rejects_zero_stale_retention() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should build");

    assert_eq!(
        membership
            .prune_stale_nodes(10, 0)
            .expect_err("zero retention must fail"),
        "error[vm_cluster_membership]: stale retention ticks must be non-zero"
    );
}

#[test]
fn vm_cluster_membership_prunes_expired_unreachable_snapshot() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should build");
    membership
        .join_peer(&peer, 1, ["worker"])
        .expect("peer should join");

    assert_eq!(
        membership.expire_stale_nodes(7),
        vec![peer.node_id().to_string()]
    );
    assert_eq!(
        membership.node(peer.node_id()).map(|node| node.state),
        Some(VmClusterNodeState::Unreachable)
    );
    assert_eq!(
        membership
            .prune_stale_nodes(7, 5)
            .expect("expired unreachable peer should prune"),
        vec![peer.node_id().to_string()]
    );
    assert!(membership.node(peer.node_id()).is_none());
}

#[test]
fn vm_cluster_membership_restart_requires_new_unfenced_incarnation() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let newer_peer = peer.next_epoch().expect("peer epoch should advance");
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should build");
    membership
        .join_peer(&peer, 1, ["search", "worker"])
        .expect("peer should join");
    membership.expire_stale_nodes(7);

    membership
        .restart_peer(&newer_peer, 8)
        .expect("new peer incarnation should restart");
    let restarted = membership.node(peer.node_id()).expect("restarted peer");
    assert_eq!(restarted.state, VmClusterNodeState::Active);
    assert_eq!(restarted.last_seen_tick, 8);
    assert_eq!(restarted.role_tags, vec!["search", "worker"]);
    assert_eq!(
        membership
            .restart_peer(&peer, 9)
            .expect_err("stale peer incarnation must fail"),
        "error[vm_cluster_membership]: stale restart epoch `7` for node `vm-b-node`; current epoch is `8`"
    );
    assert_eq!(
        membership
            .join_peer(&newer_peer, 9, ["worker"])
            .expect_err("known identity must use restart"),
        "error[vm_cluster_membership]: node `vm-b-node` is already known; use restart with a newer epoch"
    );
    let unknown = profile("other", "vm-c", "cluster-main", ["vm.message.send"])
        .next_epoch()
        .expect("unknown peer epoch should advance");
    assert_eq!(
        membership
            .restart_peer(&unknown, 9)
            .expect_err("unknown identity must fail"),
        "error[vm_cluster_membership]: cannot restart unknown node `vm-c-node`"
    );
    let mismatched_identity = VmCoordinationProfile::new(
        "other",
        "vm-x",
        peer.node_id(),
        "cluster-main",
        9,
        "0.0.7",
        ["vm.message.send"],
    )
    .expect("mismatched identity profile should build");
    assert_eq!(
        membership
            .restart_peer(&mismatched_identity, 9)
            .expect_err("mismatched stable identity must fail"),
        "error[vm_cluster_membership]: restart identity mismatch for node `vm-b-node`"
    );
    let incompatible = VmCoordinationProfile::new(
        "search",
        "vm-b",
        peer.node_id(),
        "other-cluster",
        9,
        "0.0.7",
        ["vm.message.send"],
    )
    .expect("incompatible restart profile should build");
    assert_eq!(
        membership
            .restart_peer(&incompatible, 9)
            .expect_err("incompatible restart profile must fail"),
        "error[vm_cluster_membership]: incompatible restart VM coordination profile"
    );

    membership
        .fence_node(peer.node_id(), 10)
        .expect("peer should fence");
    let newest_peer = newer_peer.next_epoch().expect("peer epoch should advance");
    assert_eq!(
        membership
            .restart_peer(&newest_peer, 11)
            .expect_err("fenced identity must not restart"),
        "error[vm_cluster_membership]: fenced node `vm-b-node` cannot restart"
    );
}

#[test]
fn vm_coordination_profile_rejects_epoch_overflow() {
    let profile = VmCoordinationProfile::new(
        "docs",
        "vm-a",
        "vm-a-node",
        "cluster-main",
        u64::MAX,
        "0.0.7",
        ["vm.message.send"],
    )
    .expect("maximum epoch profile should be valid");

    assert_eq!(
        profile
            .next_epoch()
            .expect_err("maximum epoch must not wrap"),
        "error[vm_coordination_profile]: profile epoch cannot advance beyond UInt64"
    );
}

#[test]
fn vm_cluster_membership_partition_and_heal_enforce_lifecycle() {
    let local = profile("docs", "vm-a", "cluster-main", ["vm.message.send"]);
    let peer = profile("search", "vm-b", "cluster-main", ["vm.message.send"]);
    let mut membership = VmClusterMembership::new(local, 5).expect("membership should build");
    membership
        .join_peer(&peer, 1, ["leader", "search"])
        .expect("peer should join");

    membership
        .partition_node(peer.node_id(), 2)
        .expect("active peer should partition");
    let partitioned = membership.node(peer.node_id()).expect("partitioned peer");
    assert_eq!(partitioned.state, VmClusterNodeState::Unreachable);
    assert_eq!(partitioned.last_seen_tick, 2);
    assert_eq!(partitioned.role_tags, vec!["leader", "search"]);
    assert_eq!(
        membership
            .partition_node(peer.node_id(), 3)
            .expect_err("duplicate partition must fail"),
        "error[vm_cluster_membership]: node `vm-b-node` is already partitioned"
    );
    assert_eq!(
        membership
            .heal_node(peer.node_id(), 1)
            .expect_err("stale heal must fail"),
        "error[vm_cluster_membership]: stale heal tick for node `vm-b-node`"
    );
    membership
        .heal_node(peer.node_id(), 3)
        .expect("unreachable peer should heal");
    assert_eq!(
        membership.node(peer.node_id()).map(|node| node.state),
        Some(VmClusterNodeState::Active)
    );
    assert_eq!(
        membership
            .partition_node(peer.node_id(), 2)
            .expect_err("stale partition must fail"),
        "error[vm_cluster_membership]: stale partition tick for node `vm-b-node`"
    );
    assert_eq!(
        membership
            .heal_node(peer.node_id(), 4)
            .expect_err("active peer must not heal again"),
        "error[vm_cluster_membership]: node `vm-b-node` is not heal-eligible"
    );
    assert_eq!(
        membership
            .partition_node("vm-a-node", 4)
            .expect_err("local node must not be partitioned as a peer"),
        "error[vm_cluster_membership]: local node cannot be partitioned through a peer view"
    );
    assert_eq!(
        membership
            .partition_node("missing-node", 4)
            .expect_err("unknown partition peer must fail"),
        "error[vm_cluster_membership]: unknown node `missing-node`"
    );

    let mut left = membership.clone();
    left.mark_left(peer.node_id(), 4)
        .expect("peer should leave");
    assert_eq!(
        left.partition_node(peer.node_id(), 5)
            .expect_err("left peer must not partition"),
        "error[vm_cluster_membership]: node `vm-b-node` is not partition-eligible"
    );
    let mut fenced = membership;
    fenced
        .fence_node(peer.node_id(), 4)
        .expect("peer should fence");
    assert_eq!(
        fenced
            .heal_node(peer.node_id(), 5)
            .expect_err("fenced peer must not heal"),
        "error[vm_cluster_membership]: node `vm-b-node` is not heal-eligible"
    );
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
    .expect("valid coordination test profile")
}
