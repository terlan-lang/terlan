//! Persistent application schema transitions use the same SQLite transaction owner.

use std::process::Command;

use rusqlite::Connection;

use crate::{AppendOutcome, Checkpoint, CheckpointStore, StorageError, StorageStatus};

fn checkpoint(id: &str, sequence: u64, schema: u32) -> Checkpoint {
    Checkpoint {
        id: id.into(),
        sequence,
        schema,
        payload: b"versioned logical data".to_vec(),
    }
}

#[test]
fn schema_transition_survives_reopen_without_rewriting_old_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    let old = checkpoint("old", 1, 1);
    let mut store = CheckpointStore::open(&path).unwrap();
    store.append(0, std::slice::from_ref(&old)).unwrap();
    assert_eq!(
        store.migrate_schema(1, 2).unwrap(),
        StorageStatus {
            sequence: 1,
            schema: 2,
            schema_sequence: 1
        }
    );
    store.append(1, &[checkpoint("new", 2, 2)]).unwrap();
    drop(store);
    let mut reopened = CheckpointStore::open(&path).unwrap();
    assert_eq!(
        reopened.status().unwrap(),
        StorageStatus {
            sequence: 2,
            schema: 2,
            schema_sequence: 1
        }
    );
    assert_eq!(reopened.load("old").unwrap(), Some(old.clone()));
    assert_eq!(reopened.append(0, &[old]).unwrap(), AppendOutcome::Replayed);
    assert!(matches!(
        reopened.append(2, &[checkpoint("stale-writer", 3, 1)]),
        Err(StorageError::SchemaMismatch {
            expected: 1,
            actual: 2
        })
    ));
    assert!(reopened.load("stale-writer").unwrap().is_none());
    assert_eq!(reopened.sequence().unwrap(), 2);
    reopened.compact(3).unwrap();
    assert_eq!(
        reopened.status().unwrap(),
        StorageStatus {
            sequence: 2,
            schema: 2,
            schema_sequence: 1
        }
    );
}

#[test]
fn schema_cas_rejects_competing_and_invalid_migrations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    let mut first = CheckpointStore::open(&path).unwrap();
    let mut second = CheckpointStore::open(&path).unwrap();
    first.migrate_schema(1, 7).unwrap();
    assert!(matches!(
        second.migrate_schema(1, 8),
        Err(StorageError::SchemaMismatch {
            expected: 1,
            actual: 7
        })
    ));
    for (expected, next) in [(0, 1), (7, 7), (7, 1)] {
        assert!(matches!(
            second.migrate_schema(expected, next),
            Err(StorageError::Invalid(_))
        ));
    }
    assert_eq!(
        second.status().unwrap(),
        StorageStatus {
            sequence: 0,
            schema: 7,
            schema_sequence: 0
        }
    );
    second.migrate_schema(7, u32::MAX).unwrap();
    assert_eq!(first.status().unwrap().schema, u32::MAX);
}

#[test]
fn failed_schema_transition_and_mixed_schema_batch_roll_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    let fault = Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER reject_schema BEFORE UPDATE OF payload_schema ON storage_meta BEGIN SELECT RAISE(ABORT,'injected migration failure'); END;").unwrap();
    assert!(matches!(
        store.migrate_schema(1, 2),
        Err(StorageError::Database(_))
    ));
    assert_eq!(store.status().unwrap().schema, 1);
    assert!(matches!(
        store.append(
            0,
            &[checkpoint("first", 1, 1), checkpoint("wrong-schema", 2, 2)]
        ),
        Err(StorageError::SchemaMismatch { .. })
    ));
    assert!(store.load("first").unwrap().is_none());
    assert_eq!(store.sequence().unwrap(), 0);
    fault.execute_batch("DROP TRIGGER reject_schema").unwrap();
    store.migrate_schema(1, 2).unwrap();
    store.append(0, &[checkpoint("first", 1, 2)]).unwrap();
}

#[test]
fn previous_database_format_is_rejected_without_relabelling() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    drop(CheckpointStore::open(&path).unwrap());
    let database = Connection::open(&path).unwrap();
    database.pragma_update(None, "user_version", 1).unwrap();
    assert!(matches!(
        CheckpointStore::open(&path),
        Err(StorageError::Incompatible)
    ));
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn committed_schema_survives_child_exit_without_destructors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema.sqlite");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "schema_test::schema_process_fixture"])
        .env("TERLAN_STORAGE_SCHEMA_CHILD", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(73));
    let store = CheckpointStore::open(&path).unwrap();
    assert_eq!(
        store.status().unwrap(),
        StorageStatus {
            sequence: 1,
            schema: 2,
            schema_sequence: 1
        }
    );
    assert_eq!(store.load("first").unwrap().unwrap().schema, 1);
}

/// Child-only crash point, also harmless when discovered by the ordinary harness.
#[test]
fn schema_process_fixture() {
    let Some(path) = std::env::var_os("TERLAN_STORAGE_SCHEMA_CHILD") else {
        return;
    };
    let mut store = CheckpointStore::open(std::path::Path::new(&path)).unwrap();
    store.append(0, &[checkpoint("first", 1, 1)]).unwrap();
    store.migrate_schema(1, 2).unwrap();
    std::process::exit(73);
}
