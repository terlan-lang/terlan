use super::*;
use crate::runtime::vm::coordination::{VmClusterNodeSnapshot, VmClusterNodeState};
use crate::runtime::vm::distributed_scheduler::{VmMigrationPhase, VmPlacementPolicy};

fn node(node_id: &str) -> VmClusterNodeSnapshot {
    VmClusterNodeSnapshot {
        app_id: "app".to_string(),
        vm_id: format!("vm-{node_id}"),
        node_id: node_id.to_string(),
        state: VmClusterNodeState::Active,
        last_seen_tick: 0,
        role_tags: Vec::new(),
    }
}

#[test]
fn source_snapshot_roundtrips_placement_migration_and_events() {
    let mut scheduler = VmDistributedScheduler::from_membership([node("node-a"), node("node-b")])
        .expect("scheduler");
    scheduler
        .place(
            "actor-a",
            &VmPlacementPolicy::Pinned {
                node_id: "node-b".to_string(),
            },
        )
        .expect("placement");
    scheduler
        .declare_route_policy("/jobs", VmPlacementPolicy::LeastConnections)
        .expect("route policy");
    scheduler
        .declare_actor_group_policy(
            "/jobs",
            "workers",
            VmPlacementPolicy::Pinned {
                node_id: "node-a".to_string(),
            },
        )
        .expect("actor-group policy");
    let migration = scheduler
        .request_migration("actor-a", "node-b", "node-a", true)
        .expect("migration");
    scheduler
        .advance_migration(
            &migration.actor_id,
            migration.sequence,
            VmMigrationPhase::Snapshotting,
        )
        .expect("advance");

    let snapshot = scheduler.source_snapshot();
    let restored = VmDistributedScheduler::from_source_snapshot(snapshot.clone())
        .expect("restore source snapshot");
    assert_eq!(restored.source_snapshot(), snapshot);
}

#[test]
fn source_snapshot_rejects_cursor_and_sequence_corruption() {
    let scheduler = VmDistributedScheduler::from_membership([node("node-a")]).expect("scheduler");
    let mut cursor = scheduler.source_snapshot();
    cursor.round_robin_cursor = 1;
    assert!(VmDistributedScheduler::from_source_snapshot(cursor)
        .expect_err("out-of-bounds cursor must fail")
        .contains("round-robin cursor"));

    let mut events = scheduler.source_snapshot();
    events.next_event_sequence = 1;
    assert!(VmDistributedScheduler::from_source_snapshot(events)
        .expect_err("inconsistent event sequence must fail")
        .contains("event cursor"));

    let mut overrides = scheduler.source_snapshot();
    overrides.route_policy_overrides = vec![
        ("/jobs".to_string(), VmPlacementPolicy::RoundRobin),
        ("/jobs".to_string(), VmPlacementPolicy::LeastConnections),
    ];
    assert!(VmDistributedScheduler::from_source_snapshot(overrides)
        .expect_err("duplicate route override must fail")
        .contains("duplicate route policy overrides"));
}
