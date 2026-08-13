use super::super::persistent_actor_store::{
    VmPersistentActorDurability, VmPersistentActorEvent, VmPersistentActorId,
    VmPersistentActorReplay, VmPersistentActorSchema, VmPersistentActorSnapshot,
};
use super::super::ReplValue;
use super::{
    plan_persistent_actor_compaction, VmPersistentActorCompactionCandidate,
    VmPersistentActorCompactionError, VmPersistentActorReplayEquivalence,
    VmPersistentActorRetentionPolicy,
};

#[test]
fn vm_persistent_actor_compaction_accepts_equivalent_snapshot_and_suffix() {
    let before = replay("counter-1", "Counter", 1);
    let compacted = snapshot(
        "counter-1",
        "Counter",
        1,
        2,
        4,
        ReplValue::Int(4),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted,
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence::from_snapshot(&candidate.snapshot);
    let policy = VmPersistentActorRetentionPolicy::new(4);

    let plan = plan_persistent_actor_compaction(&before, &equivalence, &candidate, &policy)
        .expect("equivalent compaction should be accepted");

    assert_eq!(plan.compacted_snapshot_generation, 2);
    assert_eq!(plan.retained_event_sequences, Vec::<u64>::new());
    assert_eq!(plan.retained_resource_handles, vec!["db.primary"]);
    assert_eq!(plan.reclaimed_resource_handles, Vec::<String>::new());
}

#[test]
fn vm_persistent_actor_compaction_rejects_schema_and_audit_floor_loss() {
    let before = replay("session-1", "Session", 1);
    let compacted = snapshot(
        "session-1",
        "Session",
        1,
        2,
        4,
        ReplValue::String("ready".to_string()),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted.clone(),
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::String("ready".to_string()),
        final_mailbox_checkpoint: compacted.mailbox_checkpoint.clone(),
        final_timer_checkpoint: compacted.timer_checkpoint.clone(),
        final_resource_handles: compacted.resource_handles.clone(),
        final_sequence: 4,
    };

    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(2).with_schema_migration_floor(3),
        ),
        Err(
            VmPersistentActorCompactionError::RetentionBeforeSchemaMigrationFloor {
                retain_from_sequence: 2,
                schema_migration_floor: 3,
            }
        )
    );
    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(2).with_audit_floor(3),
        ),
        Err(
            VmPersistentActorCompactionError::RetentionBeforeAuditFloor {
                retain_from_sequence: 2,
                audit_floor: 3,
            }
        )
    );
}

#[test]
fn vm_persistent_actor_compaction_rejects_unsafe_checkpoint_and_resource_pruning() {
    let before = replay("worker-1", "Worker", 1);
    let compacted = snapshot(
        "worker-1",
        "Worker",
        1,
        2,
        4,
        ReplValue::Int(4),
        Vec::new(),
        vec![100],
        vec!["db.primary".to_string()],
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted.clone(),
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(4),
        final_mailbox_checkpoint: Vec::new(),
        final_timer_checkpoint: compacted.timer_checkpoint.clone(),
        final_resource_handles: compacted.resource_handles.clone(),
        final_sequence: 4,
    };

    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(4),
        ),
        Err(VmPersistentActorCompactionError::MailboxCheckpointPrunedWithoutPolicy)
    );

    let compacted = snapshot(
        "worker-1",
        "Worker",
        1,
        2,
        4,
        ReplValue::Int(4),
        vec![ReplValue::String("pending".to_string())],
        Vec::new(),
        vec!["db.primary".to_string()],
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted,
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(4),
        final_mailbox_checkpoint: vec![ReplValue::String("pending".to_string())],
        final_timer_checkpoint: Vec::new(),
        final_resource_handles: vec!["db.primary".to_string()],
        final_sequence: 4,
    };

    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(4),
        ),
        Err(VmPersistentActorCompactionError::TimerCheckpointPrunedWithoutPolicy)
    );

    let compacted = snapshot(
        "worker-1",
        "Worker",
        1,
        2,
        4,
        ReplValue::Int(4),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        Vec::new(),
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted,
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(4),
        final_mailbox_checkpoint: vec![ReplValue::String("pending".to_string())],
        final_timer_checkpoint: vec![100],
        final_resource_handles: Vec::new(),
        final_sequence: 4,
    };

    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(4),
        ),
        Err(
            VmPersistentActorCompactionError::ResourceHandlePrunedWithoutPolicy {
                handle: "db.primary".to_string(),
            }
        )
    );
}

#[test]
fn vm_persistent_actor_compaction_rejects_bad_retained_event_suffix() {
    let before = replay("counter-1", "Counter", 1);
    let compacted = snapshot(
        "counter-1",
        "Counter",
        1,
        2,
        2,
        ReplValue::Int(2),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(2),
        final_mailbox_checkpoint: compacted.mailbox_checkpoint.clone(),
        final_timer_checkpoint: compacted.timer_checkpoint.clone(),
        final_resource_handles: compacted.resource_handles.clone(),
        final_sequence: 2,
    };

    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted.clone(),
        retained_events: vec![event("counter-1", "Counter", 4, ReplValue::Int(4))],
    };
    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(3),
        ),
        Err(VmPersistentActorCompactionError::RetainedEventGap {
            expected: 3,
            actual: 4,
        })
    );

    let compacted_for_unknown_event = snapshot(
        "counter-1",
        "Counter",
        1,
        2,
        4,
        ReplValue::Int(4),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    let equivalence_for_unknown_event = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(4),
        final_mailbox_checkpoint: compacted_for_unknown_event.mailbox_checkpoint.clone(),
        final_timer_checkpoint: compacted_for_unknown_event.timer_checkpoint.clone(),
        final_resource_handles: compacted_for_unknown_event.resource_handles.clone(),
        final_sequence: 4,
    };
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted_for_unknown_event,
        retained_events: vec![event("counter-1", "Counter", 5, ReplValue::Int(5))],
    };
    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence_for_unknown_event,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(5),
        ),
        Err(VmPersistentActorCompactionError::RetainedEventNotInOriginalLog { sequence: 5 })
    );
}

#[test]
fn vm_persistent_actor_compaction_rejects_non_equivalent_snapshot() {
    let before = replay("counter-1", "Counter", 1);
    let compacted = snapshot(
        "counter-1",
        "Counter",
        1,
        2,
        4,
        ReplValue::Int(999),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    let candidate = VmPersistentActorCompactionCandidate {
        snapshot: compacted,
        retained_events: Vec::new(),
    };
    let equivalence = VmPersistentActorReplayEquivalence {
        final_state: ReplValue::Int(4),
        final_mailbox_checkpoint: vec![ReplValue::String("pending".to_string())],
        final_timer_checkpoint: vec![100],
        final_resource_handles: vec!["db.primary".to_string()],
        final_sequence: 4,
    };

    assert_eq!(
        plan_persistent_actor_compaction(
            &before,
            &equivalence,
            &candidate,
            &VmPersistentActorRetentionPolicy::new(4),
        ),
        Err(VmPersistentActorCompactionError::CompactedSnapshotNotEquivalent)
    );
}

fn replay(actor: &str, schema_name: &str, schema_version: u64) -> VmPersistentActorReplay {
    let snapshot = snapshot(
        actor,
        schema_name,
        schema_version,
        1,
        1,
        ReplValue::Int(1),
        vec![ReplValue::String("pending".to_string())],
        vec![100],
        vec!["db.primary".to_string()],
    );
    VmPersistentActorReplay {
        snapshot,
        events: vec![
            event(actor, schema_name, 2, ReplValue::Int(2)),
            event(actor, schema_name, 3, ReplValue::Int(3)),
            event(actor, schema_name, 4, ReplValue::Int(4)),
        ],
    }
}
fn snapshot(
    actor: &str,
    schema_name: &str,
    schema_version: u64,
    generation: u64,
    last_event_sequence: u64,
    state: ReplValue,
    mailbox_checkpoint: Vec<ReplValue>,
    timer_checkpoint: Vec<u64>,
    resource_handles: Vec<String>,
) -> VmPersistentActorSnapshot {
    VmPersistentActorSnapshot::new(
        actor_id(actor),
        schema(schema_name, schema_version),
        generation,
        state,
        mailbox_checkpoint,
        timer_checkpoint,
        VmPersistentActorDurability {
            resource_handles: resource_handles,
            last_event_sequence: last_event_sequence,
        },
    )
    .expect("snapshot should be valid")
}

fn event(
    actor: &str,
    schema_name: &str,
    sequence: u64,
    payload: ReplValue,
) -> VmPersistentActorEvent {
    VmPersistentActorEvent::new(actor_id(actor), schema(schema_name, 1), sequence, payload)
        .expect("event should be valid")
}

fn actor_id(value: &str) -> VmPersistentActorId {
    VmPersistentActorId::new(value).expect("actor id should be valid")
}

fn schema(id: &str, version: u64) -> VmPersistentActorSchema {
    VmPersistentActorSchema::new(id, version).expect("schema should be valid")
}
