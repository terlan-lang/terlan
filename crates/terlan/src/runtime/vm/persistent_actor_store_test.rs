use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    VmDatabaseBackedPersistentActorStore, VmEmbeddedKeyValuePersistentActorStore,
    VmFileBackedPersistentActorStore, VmInMemoryPersistentActorStore, VmPersistentActorDeclaration,
    VmPersistentActorDurability, VmPersistentActorEvent, VmPersistentActorId,
    VmPersistentActorSchema, VmPersistentActorSnapshot, VmPersistentActorStoreAdapter,
    VmPersistentActorStoreOutcome,
};
use crate::runtime::vm::ReplValue;

#[test]
fn vm_persistent_actor_store_replays_snapshot_and_events_deterministically() {
    let mut store = VmInMemoryPersistentActorStore::new();
    let actor_id = actor_id("counter-1");
    let schema = schema("Counter", 1);
    let snapshot = snapshot(actor_id.clone(), schema.clone(), 1, ReplValue::Int(1), 0);

    assert_snapshot_stored(store.store_snapshot(snapshot));
    assert_event_appended(store.append_event(event(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::Int(2),
    )));
    assert_event_appended(store.append_event(event(
        actor_id.clone(),
        schema.clone(),
        2,
        ReplValue::Int(3),
    )));

    let replay = store
        .replay(&actor_id, &schema)
        .expect("snapshot replay should succeed");

    assert_eq!(replay.snapshot.state, ReplValue::Int(1));
    assert_eq!(replay.events.len(), 2);
    assert_eq!(replay.events[0].sequence, 1);
    assert_eq!(replay.events[1].sequence, 2);
}

#[test]
fn vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift() {
    let mut store = VmInMemoryPersistentActorStore::new();
    let actor_id = actor_id("session-1");
    let schema_v1 = schema("Session", 1);
    let schema_v2 = schema("Session", 2);

    assert_snapshot_stored(store.store_snapshot(snapshot(
        actor_id.clone(),
        schema_v1.clone(),
        4,
        ReplValue::String("open".to_string()),
        0,
    )));

    assert_eq!(
        store.store_snapshot(snapshot(
            actor_id.clone(),
            schema_v1.clone(),
            3,
            ReplValue::String("stale".to_string()),
            0,
        )),
        VmPersistentActorStoreOutcome::StaleSnapshot {
            actor_id: actor_id.clone(),
            current_generation: 4,
            incoming_generation: 3,
        }
    );
    assert_eq!(
        store.store_snapshot(snapshot(
            actor_id.clone(),
            schema_v2.clone(),
            5,
            ReplValue::String("migrated".to_string()),
            0,
        )),
        VmPersistentActorStoreOutcome::IncompatibleSchema {
            actor_id,
            expected: schema_v1,
            actual: schema_v2,
        }
    );
}

#[test]
fn vm_persistent_actor_store_rejects_duplicate_and_partial_events_without_mutation() {
    let mut store = VmInMemoryPersistentActorStore::new();
    let actor_id = actor_id("mailbox-1");
    let schema = schema("MailboxActor", 1);
    let first = event(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("first".to_string()),
    );
    let conflicting = event(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("different".to_string()),
    );

    assert_event_appended(store.append_event(first.clone()));
    assert_eq!(
        store.append_event(first.clone()),
        VmPersistentActorStoreOutcome::Replayed(first)
    );
    assert_eq!(
        store.append_event(conflicting),
        VmPersistentActorStoreOutcome::DuplicateEvent {
            actor_id: actor_id.clone(),
            sequence: 1,
        }
    );

    let partial = event(
        actor_id.clone(),
        schema,
        2,
        ReplValue::String("partial".to_string()),
    );
    assert_eq!(
        store.reject_partial_event(partial),
        VmPersistentActorStoreOutcome::PartialWriteRejected {
            actor_id: actor_id.clone(),
            sequence: 2,
        }
    );
    assert_eq!(store.events_after(&actor_id, 0).len(), 1);
}

#[test]
fn vm_persistent_actor_store_restores_mailbox_timer_and_resource_checkpoints() {
    let mut store = VmInMemoryPersistentActorStore::new();
    let actor_id = actor_id("worker-1");
    let schema = schema("Worker", 1);
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("ready".to_string()),
        vec![
            ReplValue::String("message-a".to_string()),
            ReplValue::String("message-b".to_string()),
        ],
        vec![10, 20],
        VmPersistentActorDurability {
            resource_handles: vec!["postgres.primary".to_string()],
            last_event_sequence: 0,
        },
    )
    .expect("checkpoint snapshot should be valid");

    assert_snapshot_stored(store.store_snapshot(snapshot));
    let replay = store
        .replay(&actor_id, &schema)
        .expect("checkpoint replay should succeed");

    assert_eq!(replay.snapshot.mailbox_checkpoint.len(), 2);
    assert_eq!(replay.snapshot.timer_checkpoint, vec![10, 20]);
    assert_eq!(replay.snapshot.resource_handles, vec!["postgres.primary"]);
}

#[test]
fn vm_persistent_actor_store_rejects_invalid_ids_schema_versions_and_handles() {
    assert_eq!(
        VmPersistentActorId::new("").expect_err("empty actor id should fail"),
        "error[vm_persistent_actor]: actor id must be non-empty"
    );
    assert_eq!(
        VmPersistentActorSchema::new("", 1).expect_err("empty schema id should fail"),
        "error[vm_persistent_actor]: schema id must be non-empty"
    );
    assert_eq!(
        VmPersistentActorSchema::new("Actor", 0).expect_err("zero schema version should fail"),
        "error[vm_persistent_actor]: schema version must be non-zero"
    );
    assert_eq!(
        VmPersistentActorSnapshot::new(
            actor_id("bad-handle"),
            schema("Actor", 1),
            1,
            ReplValue::Unit,
            Vec::new(),
            Vec::new(),
            VmPersistentActorDurability {
                resource_handles: vec!["".to_string()],
                last_event_sequence: 0,
            },
        )
        .expect_err("empty resource handle should fail"),
        "error[vm_persistent_actor]: resource handles must be non-empty"
    );
}

#[test]
fn vm_persistent_actor_declaration_binds_actor_schema_and_storage_lane() {
    let actor_id = actor_id("orders-100");
    let schema = schema("OrderActor", 1);
    let declaration = VmPersistentActorDeclaration::new(
        actor_id.clone(),
        schema.clone(),
        "store/orders-100/OrderActor",
    )
    .expect("declaration should be valid");

    assert_eq!(declaration.actor_id, actor_id);
    assert_eq!(declaration.schema, schema);
    assert_eq!(declaration.storage_lane, "store/orders-100/OrderActor");
}

#[test]
fn vm_persistent_actor_declaration_rejects_invalid_storage_lanes() {
    assert_eq!(
        VmPersistentActorDeclaration::new(actor_id("orders-100"), schema("OrderActor", 1), "")
            .expect_err("empty storage lane should fail"),
        "error[vm_persistent_actor]: persistent actor storage lane must be non-empty"
    );
    assert_eq!(
        VmPersistentActorDeclaration::new(
            actor_id("orders-100"),
            schema("OrderActor", 1),
            "store/orders-100/OtherSchema",
        )
        .expect_err("mismatched storage lane should fail"),
        "error[vm_persistent_actor]: persistent actor storage lane `store/orders-100/OtherSchema` must include actor `orders-100` and schema `OrderActor`"
    );
}

#[test]
fn vm_file_backed_persistent_actor_store_reopens_snapshot_and_events() {
    let file = temp_store_path("reopen-snapshot-and-events");
    let actor_id = actor_id("file-backed-1");
    let schema = schema("FileBackedActor", 1);

    {
        let mut store = VmFileBackedPersistentActorStore::open_file_backed(&file)
            .expect("file-backed store should open");
        assert_snapshot_stored(store.store_snapshot(snapshot(
            actor_id.clone(),
            schema.clone(),
            1,
            ReplValue::String("snapshot".to_string()),
            0,
        )));
        assert_event_appended(store.append_event(event(
            actor_id.clone(),
            schema.clone(),
            1,
            ReplValue::Atom("event".to_string()),
        )));
    }

    let reopened = VmFileBackedPersistentActorStore::open_file_backed(&file)
        .expect("file-backed store should reopen");
    let replay = reopened
        .replay(&actor_id, &schema)
        .expect("file-backed replay should succeed");

    assert_eq!(
        replay.snapshot.state,
        ReplValue::String("snapshot".to_string())
    );
    assert_eq!(replay.events.len(), 1);
    assert_eq!(
        replay.events[0].payload,
        ReplValue::Atom("event".to_string())
    );

    let _ = fs::remove_file(file);
}

#[test]
fn vm_file_backed_persistent_actor_store_rejects_corrupt_log() {
    let file = temp_store_path("rejects-corrupt-log");
    fs::write(&file, "not-a-valid-persistent-actor-record\n").expect("write corrupt log");

    let error = VmFileBackedPersistentActorStore::open_file_backed(&file)
        .expect_err("corrupt file-backed store should fail");

    assert!(error.contains("persistent actor file-backed log is corrupt"));

    let _ = fs::remove_file(file);
}

#[test]
fn vm_embedded_key_value_persistent_actor_store_exports_and_restores_snapshot_and_events() {
    let actor_id = actor_id("embedded-kv-1");
    let schema = schema("EmbeddedKeyValueActor", 1);
    let mut store = VmEmbeddedKeyValuePersistentActorStore::new_embedded_key_value();

    assert_snapshot_stored(
        store.store_snapshot(
            VmPersistentActorSnapshot::new(
                actor_id.clone(),
                schema.clone(),
                1,
                ReplValue::Int(10),
                vec![ReplValue::String("pending".to_string())],
                vec![30],
                VmPersistentActorDurability {
                    resource_handles: vec!["resource.primary".to_string()],
                    last_event_sequence: 0,
                },
            )
            .expect("snapshot should be valid"),
        ),
    );
    assert_event_appended(store.append_event(event(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::Bool(true),
    )));

    let key_values = store.export_key_values();
    assert!(key_values.keys().any(|key| key.starts_with("snapshot/")));
    assert!(key_values.keys().any(|key| key.starts_with("event/")));

    let restored = VmEmbeddedKeyValuePersistentActorStore::from_embedded_key_values(key_values)
        .expect("embedded key/value store should restore");
    let replay = restored
        .replay(&actor_id, &schema)
        .expect("embedded key/value replay should succeed");

    assert_eq!(replay.snapshot.state, ReplValue::Int(10));
    assert_eq!(
        replay.snapshot.mailbox_checkpoint,
        vec![ReplValue::String("pending".to_string())]
    );
    assert_eq!(replay.snapshot.timer_checkpoint, vec![30]);
    assert_eq!(
        replay.snapshot.resource_handles,
        vec!["resource.primary".to_string()]
    );
    assert_eq!(replay.events.len(), 1);
    assert_eq!(replay.events[0].payload, ReplValue::Bool(true));
}

#[test]
fn vm_embedded_key_value_persistent_actor_store_rejects_corrupt_records() {
    let mut key_values = BTreeMap::new();
    key_values.insert(
        "snapshot/656d6265646465642d6b762d31".to_string(),
        "not-a-valid-persistent-actor-record".to_string(),
    );

    let error = VmEmbeddedKeyValuePersistentActorStore::from_embedded_key_values(key_values)
        .expect_err("corrupt embedded key/value store should fail");

    assert!(error.contains("persistent actor embedded key/value store is corrupt"));
}

#[test]
fn vm_database_backed_persistent_actor_store_exports_sql_rows_and_replays() {
    let actor_id = actor_id("database-backed-1");
    let schema = schema("DatabaseBackedActor", 1);
    let mut store =
        VmDatabaseBackedPersistentActorStore::new_database_backed("persistent_actor_records")
            .expect("database-backed store should open");

    let statements = store.database_backed_sql_statements();
    assert!(statements
        .iter()
        .any(|statement| statement.contains("INSERT INTO persistent_actor_records")));
    assert!(statements
        .iter()
        .any(|statement| statement.contains("ORDER BY record_key")));

    assert_snapshot_stored(store.store_snapshot(snapshot(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("database-snapshot".to_string()),
        0,
    )));
    assert_event_appended(store.append_event(event(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("database-event".to_string()),
    )));

    let rows = store.export_database_rows();
    assert!(rows.keys().any(|key| key.starts_with("snapshot/")));
    assert!(rows.keys().any(|key| key.starts_with("event/")));

    let restored =
        VmDatabaseBackedPersistentActorStore::from_database_rows("persistent_actor_records", rows)
            .expect("database-backed rows should restore");
    let replay = restored
        .replay(&actor_id, &schema)
        .expect("database-backed replay should succeed");

    assert_eq!(
        replay.snapshot.state,
        ReplValue::String("database-snapshot".to_string())
    );
    assert_eq!(replay.events.len(), 1);
    assert_eq!(
        replay.events[0].payload,
        ReplValue::String("database-event".to_string())
    );
}

#[test]
fn vm_database_backed_persistent_actor_store_rejects_corrupt_rows_and_table_names() {
    assert_eq!(
        VmDatabaseBackedPersistentActorStore::new_database_backed("")
            .expect_err("empty table should fail"),
        "error[vm_persistent_actor]: persistent actor database table must be non-empty"
    );
    assert_eq!(
        VmDatabaseBackedPersistentActorStore::new_database_backed("bad-table")
            .expect_err("unsafe table should fail"),
        "error[vm_persistent_actor]: persistent actor database table `bad-table` must use identifier characters"
    );

    let mut rows = BTreeMap::new();
    rows.insert(
        "snapshot/64617461626173652d6261636b65642d31".to_string(),
        "not-a-valid-persistent-actor-record".to_string(),
    );

    let error =
        VmDatabaseBackedPersistentActorStore::from_database_rows("persistent_actor_records", rows)
            .expect_err("corrupt database-backed rows should fail");

    assert!(error.contains("persistent actor database-backed row is corrupt"));
}

fn assert_snapshot_stored(outcome: VmPersistentActorStoreOutcome) {
    assert!(matches!(
        outcome,
        VmPersistentActorStoreOutcome::SnapshotStored(_)
    ));
}

fn assert_event_appended(outcome: VmPersistentActorStoreOutcome) {
    assert!(matches!(
        outcome,
        VmPersistentActorStoreOutcome::EventAppended(_)
    ));
}

fn actor_id(value: &str) -> VmPersistentActorId {
    VmPersistentActorId::new(value).expect("actor id should be valid")
}

fn schema(id: &str, version: u64) -> VmPersistentActorSchema {
    VmPersistentActorSchema::new(id, version).expect("schema should be valid")
}

fn snapshot(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
    generation: u64,
    state: ReplValue,
    last_event_sequence: u64,
) -> VmPersistentActorSnapshot {
    VmPersistentActorSnapshot::new(
        actor_id,
        schema,
        generation,
        state,
        Vec::new(),
        Vec::new(),
        VmPersistentActorDurability {
            resource_handles: Vec::new(),
            last_event_sequence: last_event_sequence,
        },
    )
    .expect("snapshot should be valid")
}

fn event(
    actor_id: VmPersistentActorId,
    schema: VmPersistentActorSchema,
    sequence: u64,
    payload: ReplValue,
) -> VmPersistentActorEvent {
    VmPersistentActorEvent::new(actor_id, schema, sequence, payload).expect("event should be valid")
}

fn temp_store_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-vm-persistent-actor-store-{name}-{}-{unique}.log",
        std::process::id()
    ))
}
