use super::*;

#[test]
fn vm_distributed_scheduler_round_robin_uses_active_nodes_in_deterministic_order() {
    let mut scheduler = VmDistributedScheduler::from_membership([
        node("node-b", VmClusterNodeState::Active),
        node("node-a", VmClusterNodeState::Active),
        node("node-c", VmClusterNodeState::Unreachable),
    ])
    .expect("scheduler should build");

    assert_eq!(scheduler.active_node_count(), 2);
    assert_eq!(
        scheduler
            .place("actor-1", &VmPlacementPolicy::RoundRobin)
            .expect("first placement"),
        decision("actor-1", "node-a", "round_robin", false)
    );
    assert_eq!(
        scheduler
            .place("actor-2", &VmPlacementPolicy::RoundRobin)
            .expect("second placement"),
        decision("actor-2", "node-b", "round_robin", false)
    );
    assert_eq!(
        scheduler
            .place("actor-3", &VmPlacementPolicy::RoundRobin)
            .expect("third placement"),
        decision("actor-3", "node-a", "round_robin", false)
    );
}

#[test]
fn vm_distributed_scheduler_least_connections_uses_stable_tie_breaks() {
    let mut scheduler = VmDistributedScheduler::from_membership([
        node("node-b", VmClusterNodeState::Active),
        node("node-a", VmClusterNodeState::Active),
    ])
    .expect("scheduler should build");

    assert_eq!(
        scheduler
            .place("actor-1", &VmPlacementPolicy::LeastConnections)
            .expect("tie placement"),
        decision("actor-1", "node-a", "least_connections", false)
    );
    scheduler
        .update_load("node-a", 4)
        .expect("node-a load should update");
    scheduler
        .update_load("node-b", 1)
        .expect("node-b load should update");

    assert_eq!(
        scheduler
            .place("actor-2", &VmPlacementPolicy::LeastConnections)
            .expect("least-loaded placement"),
        decision("actor-2", "node-b", "least_connections", false)
    );
}

#[test]
fn vm_distributed_scheduler_membership_refresh_preserves_surviving_load_and_cursor() {
    let mut scheduler = VmDistributedScheduler::from_membership([
        node("node-a", VmClusterNodeState::Active),
        node("node-b", VmClusterNodeState::Active),
    ])
    .expect("scheduler should build");
    scheduler
        .update_load("node-b", 7)
        .expect("node-b load should update");

    assert_eq!(
        scheduler
            .place("actor-1", &VmPlacementPolicy::RoundRobin)
            .expect("first round-robin placement"),
        decision("actor-1", "node-a", "round_robin", false)
    );
    assert_eq!(
        scheduler
            .place("actor-2", &VmPlacementPolicy::RoundRobin)
            .expect("second round-robin placement"),
        decision("actor-2", "node-b", "round_robin", false)
    );

    scheduler
        .refresh_membership([
            node("node-a", VmClusterNodeState::Unreachable),
            node("node-b", VmClusterNodeState::Active),
        ])
        .expect("membership should refresh");

    assert_eq!(scheduler.active_node_count(), 1);
    assert_eq!(
        scheduler
            .place("actor-3", &VmPlacementPolicy::RoundRobin)
            .expect("cursor should stay valid after membership shrink"),
        decision("actor-3", "node-b", "round_robin", false)
    );
    assert_eq!(
        scheduler
            .place("actor-4", &VmPlacementPolicy::LeastConnections)
            .expect("surviving load should remain placeable"),
        decision("actor-4", "node-b", "least_connections", false)
    );

    scheduler
        .refresh_membership([
            node("node-a", VmClusterNodeState::Active),
            node("node-b", VmClusterNodeState::Active),
        ])
        .expect("membership should add node-a back");
    assert_eq!(
        scheduler
            .place("actor-5", &VmPlacementPolicy::LeastConnections)
            .expect("newly active node should start with zero load"),
        decision("actor-5", "node-a", "least_connections", false)
    );
}

#[test]
fn vm_distributed_scheduler_emits_typed_placement_events_with_cursor_reads() {
    let mut scheduler = two_node_scheduler();

    let first = scheduler
        .place("actor-1", &VmPlacementPolicy::RoundRobin)
        .expect("first placement");
    let second = scheduler
        .place("actor-2", &VmPlacementPolicy::LeastConnections)
        .expect("second placement");

    assert_eq!(first.node_id, "node-a");
    assert_eq!(second.node_id, "node-a");
    assert_eq!(scheduler.events().len(), 2);
    assert_eq!(scheduler.events()[0].event_sequence, 1);
    assert_eq!(scheduler.events()[0].actor_id, "actor-1");
    assert_eq!(
        scheduler.events()[0].kind,
        VmSchedulerEventKind::Placement {
            node_id: "node-a".to_string(),
            policy: "round_robin",
            fallback_used: false,
        }
    );
    assert_eq!(scheduler.events_after(1).len(), 1);
    assert_eq!(scheduler.events_after(2), Vec::new());
    assert_eq!(
        scheduler.placement_assignment("actor-1"),
        Some(&assignment("actor-1", "node-a", "round_robin", false, 1))
    );
    assert_eq!(
        scheduler.placement_assignment("actor-2"),
        Some(&assignment(
            "actor-2",
            "node-a",
            "least_connections",
            false,
            2
        ))
    );
}

#[test]
fn vm_distributed_scheduler_applies_monotonic_remote_placement_updates() {
    let mut scheduler = two_node_scheduler();

    let applied = scheduler
        .apply_placement_update(placement_event(
            3,
            "actor-1",
            "node-b",
            "remote_shard",
            true,
        ))
        .expect("remote placement should apply");

    assert_eq!(
        applied,
        assignment("actor-1", "node-b", "remote_shard", true, 3)
    );
    assert_eq!(
        scheduler.placement_assignment("actor-1"),
        Some(&assignment("actor-1", "node-b", "remote_shard", true, 3))
    );
    let newer = scheduler
        .apply_placement_update(placement_event(
            4,
            "actor-1",
            "node-a",
            "remote_rebalance",
            false,
        ))
        .expect("newer placement should apply");
    assert_eq!(
        newer,
        assignment("actor-1", "node-a", "remote_rebalance", false, 4)
    );
}

#[test]
fn vm_distributed_scheduler_rejects_stale_and_conflicting_placement_updates() {
    let mut scheduler = two_node_scheduler();
    scheduler
        .apply_placement_update(placement_event(5, "actor-1", "node-a", "remote", false))
        .expect("initial remote placement should apply");

    let replay = scheduler
        .apply_placement_update(placement_event(5, "actor-1", "node-a", "remote", false))
        .expect("identical placement update should replay");
    assert_eq!(replay, assignment("actor-1", "node-a", "remote", false, 5));

    let stale = scheduler
        .apply_placement_update(placement_event(4, "actor-1", "node-b", "remote", false))
        .expect_err("stale placement should fail");
    assert_eq!(
        stale,
        "error[vm_distributed_scheduler]: stale placement update sequence `4` is older than current sequence `5` for actor `actor-1`"
    );
    assert_eq!(
        scheduler.failure_envelopes_after(0),
        vec![failure_envelope(
            "node-b",
            4,
            VmDistributedFailureKind::StalePlacementUpdate {
                actor_id: "actor-1".to_string(),
                incoming_sequence: 4,
                current_sequence: 5,
            },
            &stale,
        )]
    );
    scheduler
        .apply_placement_update(placement_event(4, "actor-1", "node-b", "remote", false))
        .expect_err("duplicate stale placement should remain rejected");
    assert_eq!(scheduler.failure_envelopes_after(0).len(), 1);
    let conflict = scheduler
        .apply_placement_update(placement_event(
            5,
            "actor-1",
            "node-b",
            "remote_conflict",
            false,
        ))
        .expect_err("conflicting placement should fail");
    assert_eq!(
        conflict,
        "error[vm_distributed_scheduler]: conflicting placement update sequence `5` for actor `actor-1`"
    );
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_remote_placement_updates() {
    let mut scheduler = two_node_scheduler();

    let zero = scheduler
        .apply_placement_update(placement_event(0, "actor-1", "node-a", "remote", false))
        .expect_err("zero placement sequence should fail");
    assert_eq!(
        zero,
        "error[vm_distributed_scheduler]: placement update sequence must be non-zero"
    );
    let non_placement = scheduler
        .apply_placement_update(VmSchedulerEvent {
            event_sequence: 6,
            actor_id: "actor-1".to_string(),
            kind: VmSchedulerEventKind::MigrationCommitted {
                migration_sequence: 1,
            },
        })
        .expect_err("non-placement event should fail");
    assert_eq!(
        non_placement,
        "error[vm_distributed_scheduler]: event sequence `6` is not a placement update"
    );
    let inactive = scheduler
        .apply_placement_update(placement_event(6, "actor-1", "node-c", "remote", false))
        .expect_err("inactive node should fail");
    assert_eq!(
        inactive,
        "error[vm_distributed_scheduler]: placement update node `node-c` is not active"
    );
}

#[test]
fn vm_distributed_scheduler_rejects_invalid_pinned_and_empty_inputs() {
    let mut scheduler =
        VmDistributedScheduler::from_membership([node("node-a", VmClusterNodeState::Active)])
            .expect("scheduler should build");

    let empty_actor_error = scheduler
        .place("", &VmPlacementPolicy::RoundRobin)
        .expect_err("empty actor should fail");
    assert_eq!(
        empty_actor_error,
        "error[vm_distributed_scheduler]: actor id must be non-empty"
    );
    let pinned_error = scheduler
        .place(
            "actor-1",
            &VmPlacementPolicy::Pinned {
                node_id: "node-b".to_string(),
            },
        )
        .expect_err("unknown pinned node should fail");
    assert_eq!(
        pinned_error,
        "error[vm_distributed_scheduler]: pinned node `node-b` is not active"
    );
    let load_error = scheduler
        .update_load("node-b", 1)
        .expect_err("unknown load update should fail");
    assert_eq!(
        load_error,
        "error[vm_distributed_scheduler]: unknown active node `node-b`"
    );
    let empty_cluster_error =
        VmDistributedScheduler::from_membership([node("node-a", VmClusterNodeState::Left)])
            .expect_err("empty active membership should fail");
    assert_eq!(
        empty_cluster_error,
        "error[vm_distributed_scheduler]: no active nodes available"
    );
}

#[test]
fn vm_distributed_scheduler_shard_affinity_is_stable_and_explicitly_falls_back() {
    let mut scheduler = VmDistributedScheduler::from_membership([
        node("node-a", VmClusterNodeState::Active),
        node("node-b", VmClusterNodeState::Active),
    ])
    .expect("scheduler should build");
    let policy = VmPlacementPolicy::ShardAffinity {
        shard_key: "tenant-1".to_string(),
        fallback: VmPlacementFallback::RoundRobin,
    };

    let first = scheduler
        .place("actor-1", &policy)
        .expect("initial shard placement");
    let second = scheduler
        .place("actor-2", &policy)
        .expect("stable shard placement");

    assert_eq!(first.node_id, "node-a");
    assert_eq!(first.fallback_used, true);
    assert_eq!(second.node_id, "node-a");
    assert_eq!(second.fallback_used, false);
    assert_eq!(scheduler.shard_owner("tenant-1"), Some("node-a"));

    scheduler
        .refresh_membership([
            node("node-a", VmClusterNodeState::Unreachable),
            node("node-b", VmClusterNodeState::Active),
        ])
        .expect("membership should refresh");
    let fallback = scheduler
        .place("actor-3", &policy)
        .expect("fallback should choose active node");
    assert_eq!(fallback.node_id, "node-b");
    assert_eq!(fallback.fallback_used, true);
    assert_eq!(scheduler.shard_owner("tenant-1"), Some("node-b"));
}

#[test]
fn vm_distributed_scheduler_shard_affinity_can_reject_unavailable_owner() {
    let mut scheduler = VmDistributedScheduler::from_membership([
        node("node-a", VmClusterNodeState::Active),
        node("node-b", VmClusterNodeState::Active),
    ])
    .expect("scheduler should build");
    let policy = VmPlacementPolicy::ShardAffinity {
        shard_key: "tenant-1".to_string(),
        fallback: VmPlacementFallback::Reject,
    };

    scheduler
        .place("actor-1", &policy)
        .expect("initial shard placement");
    scheduler
        .refresh_membership([
            node("node-a", VmClusterNodeState::Fenced),
            node("node-b", VmClusterNodeState::Active),
        ])
        .expect("membership should refresh");
    let error = scheduler
        .place("actor-2", &policy)
        .expect_err("reject fallback should fail");

    assert_eq!(
        error,
        "error[vm_distributed_scheduler]: shard `tenant-1` owner `node-a` is not active"
    );
}

fn placement_event(
    event_sequence: u64,
    actor_id: &str,
    node_id: &str,
    policy: &'static str,
    fallback_used: bool,
) -> VmSchedulerEvent {
    VmSchedulerEvent {
        event_sequence,
        actor_id: actor_id.to_string(),
        kind: VmSchedulerEventKind::Placement {
            node_id: node_id.to_string(),
            policy,
            fallback_used,
        },
    }
}
