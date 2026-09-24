//! Persistent identity distinguishes logical stores without claiming authentication.

use crate::{Checkpoint, CheckpointStore, StorageError};
use rusqlite::Connection;

fn checkpoint() -> Checkpoint {
    Checkpoint {
        id: "checkpoint".into(),
        sequence: 1,
        schema: 1,
        payload: b"logical state".to_vec(),
    }
}

#[test]
fn identity_survives_transactions_reopen_and_storage_relocation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    let initial = store.observation().unwrap();
    assert_ne!(initial.identity, [0; 32]);
    store.append(0, &[checkpoint()]).unwrap();
    store.migrate_schema(1, 2).unwrap();
    store.compact(2).unwrap();
    let observed = store.observation().unwrap();
    assert_eq!(initial.identity, observed.identity);
    assert_eq!(observed.status, store.status().unwrap());
    assert_eq!(observed.status.sequence, 1);
    assert_eq!(observed.status.schema, 2);
    store.flush().unwrap();
    drop(store);
    let moved = directory.path().join("moved.sqlite");
    std::fs::rename(path, &moved).unwrap();
    let reopened = CheckpointStore::open(&moved).unwrap();
    assert_eq!(reopened.observation().unwrap(), observed);
    let separate = CheckpointStore::open(&directory.path().join("independent.sqlite")).unwrap();
    assert_ne!(separate.observation().unwrap().identity, observed.identity);
}

#[test]
fn format_two_is_upgraded_atomically_without_losing_data_or_schema() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("previous.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    store.append(0, &[checkpoint()]).unwrap();
    store.migrate_schema(1, 2).unwrap();
    store.flush().unwrap();
    drop(store);
    // Reconstruct the exact preceding format's tables, retaining its committed data.
    let database = Connection::open(&path).unwrap();
    database
        .execute_batch("DROP TABLE storage_identity; PRAGMA user_version=2;")
        .unwrap();
    drop(database);
    let upgraded = CheckpointStore::open(&path).unwrap();
    let observed = upgraded.observation().unwrap();
    assert_eq!(observed.status.schema, 2);
    assert_eq!(observed.status.sequence, 1);
    assert_eq!(observed.status.schema_sequence, 1);
    assert_eq!(upgraded.load("checkpoint").unwrap(), Some(checkpoint()));
    drop(upgraded);
    assert_eq!(
        CheckpointStore::open(&path).unwrap().observation().unwrap(),
        observed
    );
    let database = Connection::open(path).unwrap();
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        3
    );
}

#[test]
fn invalid_previous_metadata_is_not_relabelled_or_given_an_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("previous.sqlite");
    drop(CheckpointStore::open(&path).unwrap());
    let database = Connection::open(&path).unwrap();
    database.execute_batch("DROP TABLE storage_identity; PRAGMA user_version=2; PRAGMA ignore_check_constraints=ON; UPDATE storage_meta SET highest_sequence=-1;").unwrap();
    assert!(matches!(
        CheckpointStore::open(&path),
        Err(StorageError::Corrupt)
    ));
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        database
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='storage_identity'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn corrupt_missing_and_zero_identities_are_never_regenerated() {
    for statement in [
        "UPDATE storage_identity SET identity=x'01'",
        "UPDATE storage_identity SET identity=zeroblob(32)",
        "DELETE FROM storage_identity",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt.sqlite");
        drop(CheckpointStore::open(&path).unwrap());
        let database = Connection::open(&path).unwrap();
        database
            .execute_batch("PRAGMA ignore_check_constraints=ON")
            .unwrap();
        database.execute_batch(statement).unwrap();
        assert!(matches!(
            CheckpointStore::open(&path),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(
            database
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
}
