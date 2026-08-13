use super::super::model_sync::{
    VmModelSyncChange, VmModelSyncChangeKind, VmModelSyncKey, VmModelSyncVersion,
};
use super::super::persistent_actor_store::{
    VmDatabaseBackedPersistentActorStore, VmPersistentActorDurability, VmPersistentActorEvent,
    VmPersistentActorId, VmPersistentActorSchema, VmPersistentActorSnapshot,
    VmPersistentActorStoreAdapter,
};
use super::super::ReplValue;
use super::{
    build_cross_machine_actor_export, execute_persistent_actor_restore,
    generate_minimal_actor_replay_fixture, plan_persistent_actor_restore, VmPersistentActorExport,
    VmPersistentActorModelSyncContinuity, VmPersistentActorRestoreCapabilities,
    VmPersistentActorRestoreError, VmPersistentActorRestoreTarget,
};

fn actor_id(value: &str) -> VmPersistentActorId {
    VmPersistentActorId::new(value).expect("actor id")
}

fn schema(version: u64) -> VmPersistentActorSchema {
    VmPersistentActorSchema::new("cart", version).expect("schema")
}

fn snapshot(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
) -> VmPersistentActorSnapshot {
    VmPersistentActorSnapshot::new(
        actor_id,
        schema,
        2,
        ReplValue::String("state-v2".to_string()),
        vec![ReplValue::Atom("pending_message".to_string())],
        vec![30, 40],
        VmPersistentActorDurability {
            resource_handles: vec!["db-session".to_string()],
            last_event_sequence: 10,
        },
    )
    .expect("snapshot")
}

fn event(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
    sequence: u64,
) -> VmPersistentActorEvent {
    VmPersistentActorEvent::new(
        actor_id,
        schema,
        sequence,
        ReplValue::String(format!("event-{sequence}")),
    )
    .expect("event")
}

fn model_sync_change(
    sequence: u64,
    model: &str,
    id: &str,
    version_sequence: u64,
) -> VmModelSyncChange {
    VmModelSyncChange {
        sequence,
        key: VmModelSyncKey::new(model, id).expect("model sync key"),
        version: VmModelSyncVersion::new(version_sequence, "node-a").expect("model sync version"),
        kind: VmModelSyncChangeKind::Updated,
        value: Some(ReplValue::String(format!("{model}-{id}-{sequence}"))),
    }
}

fn mailbox_checkpoint(sequence: i64, payload: &str) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("mailbox_checkpoint".to_string()),
        ReplValue::Int(sequence),
        ReplValue::Atom(payload.to_string()),
    ])
}

fn export_for(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
) -> VmPersistentActorExport {
    VmPersistentActorExport::new(
        snapshot(actor_id.clone(), schema.clone()),
        vec![
            event(actor_id.clone(), schema.clone(), 11),
            event(actor_id, schema, 12),
        ],
        vec!["secret_token".to_string()],
        false,
    )
    .expect("export")
}

fn target_for(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
) -> VmPersistentActorRestoreTarget {
    VmPersistentActorRestoreTarget::new(
        actor_id,
        schema,
        ["db-session".to_string()],
        VmPersistentActorRestoreCapabilities::full(),
    )
}

#[test]
fn vm_persistent_actor_restore_accepts_deterministic_export_plan() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone());
    let target = target_for(actor, schema);

    let plan = plan_persistent_actor_restore(&export, &target).expect("restore plan");

    assert_eq!(plan.snapshot_generation, 2);
    assert_eq!(plan.restored_event_sequences, vec![11, 12]);
    assert_eq!(plan.restored_resource_handles, vec!["db-session"]);
    assert_eq!(plan.redacted_fields, vec!["secret_token"]);
}

#[test]
fn vm_persistent_actor_restore_rejects_corrupt_export_and_stale_schema() {
    let actor = actor_id("cart-1");
    let schema_v2 = schema(2);
    let export = export_for(actor.clone(), schema_v2.clone()).with_checksum("tampered");
    let target = target_for(actor.clone(), schema_v2.clone());

    assert_eq!(
        plan_persistent_actor_restore(&export, &target),
        Err(VmPersistentActorRestoreError::CorruptExportChecksum)
    );

    let stale_target = target_for(actor, schema(1));
    let export = export_for(actor_id("cart-1"), schema_v2);

    assert_eq!(
        plan_persistent_actor_restore(&export, &stale_target),
        Err(VmPersistentActorRestoreError::StaleSchema)
    );
}

#[test]
fn vm_persistent_actor_restore_rejects_wrong_actor_and_missing_resource() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone());
    let wrong_actor_target = target_for(actor_id("cart-2"), schema.clone());

    assert_eq!(
        plan_persistent_actor_restore(&export, &wrong_actor_target),
        Err(VmPersistentActorRestoreError::WrongActorOwner)
    );

    let missing_resource_target = VmPersistentActorRestoreTarget::new(
        actor,
        schema,
        Vec::<String>::new(),
        VmPersistentActorRestoreCapabilities::full(),
    );

    assert_eq!(
        plan_persistent_actor_restore(&export, &missing_resource_target),
        Err(
            VmPersistentActorRestoreError::MissingDurableResourceHandle {
                handle: "db-session".to_string()
            }
        )
    );
}

#[test]
fn vm_persistent_actor_restore_rejects_reordered_event_suffix() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = VmPersistentActorExport::new(
        snapshot(actor.clone(), schema.clone()),
        vec![event(actor.clone(), schema.clone(), 12)],
        Vec::new(),
        false,
    );

    assert_eq!(
        export,
        Err(
            VmPersistentActorRestoreError::ReorderedRetainedEventSuffix {
                expected: 11,
                actual: 12
            }
        )
    );
}

#[test]
fn vm_persistent_actor_restore_rejects_reordered_mailbox_checkpoint() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let snapshot = VmPersistentActorSnapshot::new(
        actor.clone(),
        schema.clone(),
        2,
        ReplValue::String("state-v2".to_string()),
        vec![
            mailbox_checkpoint(1, "first"),
            mailbox_checkpoint(3, "third"),
        ],
        vec![30],
        VmPersistentActorDurability {
            resource_handles: vec!["db-session".to_string()],
            last_event_sequence: 10,
        },
    )
    .expect("snapshot");

    assert_eq!(
        VmPersistentActorExport::new(snapshot, vec![event(actor, schema, 11)], Vec::new(), false,),
        Err(VmPersistentActorRestoreError::ReorderedMailboxCheckpoint {
            expected: 2,
            actual: 3
        })
    );
}

#[test]
fn vm_persistent_actor_restore_gates_compacted_snapshot_and_resource_adapter_support() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let compacted_export = VmPersistentActorExport::new(
        snapshot(actor.clone(), schema.clone()),
        vec![event(actor.clone(), schema.clone(), 11)],
        Vec::new(),
        true,
    )
    .expect("compacted export");
    let compacted_target = VmPersistentActorRestoreTarget::new(
        actor.clone(),
        schema.clone(),
        ["db-session".to_string()],
        VmPersistentActorRestoreCapabilities::without_compaction(),
    );

    assert_eq!(
        plan_persistent_actor_restore(&compacted_export, &compacted_target),
        Err(VmPersistentActorRestoreError::IncompatibleAdapterForCompactedSnapshot)
    );

    let resource_target = VmPersistentActorRestoreTarget::new(
        actor,
        schema,
        ["db-session".to_string()],
        VmPersistentActorRestoreCapabilities::without_resource_handles(),
    );

    assert_eq!(
        plan_persistent_actor_restore(&compacted_export, &resource_target),
        Err(VmPersistentActorRestoreError::IncompatibleAdapterForResourceHandles)
    );
}

#[test]
fn vm_persistent_actor_restore_rejects_incompatible_adapter_kind() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone()).with_source_adapter_kind("force_local");
    let target = target_for(actor, schema).with_adapter_kind("cluster");

    assert_eq!(
        plan_persistent_actor_restore(&export, &target),
        Err(VmPersistentActorRestoreError::IncompatibleAdapterKind {
            expected: "force_local".to_string(),
            actual: "cluster".to_string()
        })
    );
}

#[test]
fn vm_persistent_actor_restore_executes_cross_adapter_restore() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export =
        export_for(actor.clone(), schema.clone()).with_source_adapter_kind("embedded-key-value");
    let target = target_for(actor.clone(), schema.clone())
        .with_adapter_kind("database-backed")
        .allow_cross_adapter_restore();
    let mut destination =
        VmDatabaseBackedPersistentActorStore::new_database_backed("persistent_actor_restore")
            .expect("database-backed store");

    let execution = execute_persistent_actor_restore(&export, &target, &mut destination)
        .expect("cross-adapter restore execution");

    assert_eq!(execution.source_adapter_kind, "embedded-key-value");
    assert_eq!(execution.destination_adapter_kind, "database-backed");
    assert_eq!(execution.snapshot_generation, 2);
    assert_eq!(execution.restored_event_count, 2);
    assert_eq!(execution.replayed_event_count, 2);
    assert_eq!(
        destination
            .replay(&actor, &schema)
            .expect("restored actor replay")
            .events
            .len(),
        2
    );
    assert_eq!(destination.export_database_rows().len(), 3);

    assert_eq!(
        execute_persistent_actor_restore(&export, &target, &mut destination),
        Err(VmPersistentActorRestoreError::StoreRejected {
            step: "store_snapshot",
            outcome: "stale_snapshot"
        })
    );
}

#[test]
fn vm_persistent_actor_restore_builds_cross_machine_export_format() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor, schema).with_model_sync_changes(vec![
        model_sync_change(5, "User", "alice", 2),
        model_sync_change(6, "User", "bob", 1),
    ]);

    let envelope =
        build_cross_machine_actor_export(&export, "node-a.us-east").expect("portable envelope");

    assert_eq!(
        envelope.format_version,
        "terlan-vm-persistent-actor-export-v1"
    );
    assert_eq!(envelope.source_machine_id, "node-a.us-east");
    assert_eq!(envelope.actor_id, "cart-1");
    assert_eq!(envelope.schema_id, "cart");
    assert_eq!(envelope.schema_version, 2);
    assert_eq!(envelope.snapshot_generation, 2);
    assert_eq!(envelope.snapshot_last_event_sequence, 10);
    assert_eq!(envelope.retained_event_sequences, vec![11, 12]);
    assert_eq!(envelope.resource_handle_count, 1);
    assert_eq!(envelope.redacted_fields, vec!["secret_token"]);
    assert_eq!(envelope.model_sync_streams, vec!["User:5-6#2"]);
    assert_eq!(envelope.export_checksum, export.checksum);

    let manifest = envelope.render_manifest();
    assert!(manifest.contains("format=terlan-vm-persistent-actor-export-v1"));
    assert!(manifest.contains("source_machine=node-a.us-east"));
    assert!(manifest.contains("events=11,12"));
    assert!(manifest.contains("model_sync_streams=User:5-6#2"));
    assert!(!manifest.contains("state-v2"));
    assert!(!manifest.contains("pending_message"));
    assert!(!manifest.contains("event-11"));

    assert_eq!(
        build_cross_machine_actor_export(&export, "../bad-node"),
        Err(
            VmPersistentActorRestoreError::InvalidCrossMachineExportSource {
                source_machine_id: "../bad-node".to_string()
            }
        )
    );
}

#[test]
fn vm_persistent_actor_restore_accepts_compacted_export_with_restore_boundary() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = VmPersistentActorExport::new(
        snapshot(actor.clone(), schema.clone()),
        vec![
            event(actor.clone(), schema.clone(), 11),
            event(actor.clone(), schema.clone(), 12),
        ],
        Vec::new(),
        true,
    )
    .expect("compacted export");
    let target = target_for(actor, schema);

    let plan = plan_persistent_actor_restore(&export, &target).expect("restore plan");
    let compaction = plan
        .compaction_restore
        .as_ref()
        .expect("compaction restore metadata");

    assert_eq!(compaction.compacted_snapshot_generation, 2);
    assert_eq!(compaction.compacted_through_sequence, 10);
    assert_eq!(compaction.retained_suffix_start, Some(11));
    assert_eq!(compaction.retained_suffix_end, Some(12));

    let fixture = generate_minimal_actor_replay_fixture(&export, &target).expect("fixture");
    assert_eq!(fixture.compacted_through_sequence, Some(10));
    assert_eq!(fixture.retained_suffix_start, Some(11));
    assert!(fixture.render_manifest().contains("compacted_through=10"));
    assert!(fixture
        .render_manifest()
        .contains("retained_suffix_start=11"));
}

#[test]
fn vm_persistent_actor_restore_validates_model_sync_stream_continuity() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone()).with_model_sync_changes(vec![
        model_sync_change(5, "User", "alice", 2),
        model_sync_change(6, "User", "bob", 1),
        model_sync_change(1, "Session", "s1", 1),
    ]);
    let target = target_for(actor, schema).with_required_model_sync_streams(vec![
        VmPersistentActorModelSyncContinuity::new("User", 5),
    ]);

    let plan = plan_persistent_actor_restore(&export, &target).expect("restore plan");

    assert_eq!(plan.model_sync_streams.len(), 1);
    assert_eq!(plan.model_sync_streams[0].model, "User");
    assert_eq!(plan.model_sync_streams[0].retained_from_sequence, 5);
    assert_eq!(plan.model_sync_streams[0].retained_to_sequence, 6);
    assert_eq!(plan.model_sync_streams[0].change_count, 2);

    let fixture = generate_minimal_actor_replay_fixture(&export, &target).expect("fixture");
    assert_eq!(fixture.model_sync_streams, vec!["User:5-6#2"]);
    assert!(fixture
        .render_manifest()
        .contains("model_sync_streams=User:5-6#2"));
}

#[test]
fn vm_persistent_actor_restore_rejects_missing_and_reordered_model_sync_stream() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone());
    let missing_target = target_for(actor.clone(), schema.clone())
        .with_required_model_sync_streams(vec![VmPersistentActorModelSyncContinuity::new(
            "User", 5,
        )]);

    assert_eq!(
        plan_persistent_actor_restore(&export, &missing_target),
        Err(VmPersistentActorRestoreError::MissingModelSyncContinuity {
            model: "User".to_string()
        })
    );

    let gapped_export = export_for(actor.clone(), schema.clone())
        .with_model_sync_changes(vec![model_sync_change(6, "User", "alice", 2)]);
    let gapped_target = target_for(actor, schema).with_required_model_sync_streams(vec![
        VmPersistentActorModelSyncContinuity::new("User", 5),
    ]);

    assert_eq!(
        plan_persistent_actor_restore(&gapped_export, &gapped_target),
        Err(VmPersistentActorRestoreError::ReorderedModelSyncStream {
            expected: 5,
            actual: 6
        })
    );
}

#[test]
fn vm_persistent_actor_restore_generates_minimal_replay_fixture_without_payloads() {
    let actor = actor_id("cart-1");
    let schema = schema(2);
    let export = export_for(actor.clone(), schema.clone());
    let target = target_for(actor, schema);

    let fixture = generate_minimal_actor_replay_fixture(&export, &target).expect("replay fixture");

    assert_eq!(fixture.actor_id, "cart-1");
    assert_eq!(fixture.schema_id, "cart");
    assert_eq!(fixture.schema_version, 2);
    assert_eq!(fixture.snapshot_generation, 2);
    assert_eq!(fixture.snapshot_last_event_sequence, 10);
    assert_eq!(fixture.retained_event_sequences, vec![11, 12]);
    assert_eq!(fixture.mailbox_checkpoint_count, 1);
    assert!(fixture.mailbox_checkpoint_sequences.is_empty());
    assert_eq!(fixture.timer_deadlines, vec![30, 40]);
    assert_eq!(fixture.resource_handles, vec!["db-session"]);
    assert_eq!(fixture.redacted_fields, vec!["secret_token"]);
    assert_eq!(fixture.export_checksum, export.checksum);

    let manifest = fixture.render_manifest();
    assert!(manifest.contains("actor=cart-1"));
    assert!(manifest.contains("schema=cart:2"));
    assert!(manifest.contains("events=11,12"));
    assert!(!manifest.contains("state-v2"));
    assert!(!manifest.contains("pending_message"));
    assert!(!manifest.contains("event-11"));
    assert!(!manifest.contains("event-12"));
}
