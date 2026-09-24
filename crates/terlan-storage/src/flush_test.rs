//! Real WAL checkpoint completion, contention, and recovery without fake counters.

use rusqlite::Connection;

use crate::{Checkpoint, CheckpointStore, StorageError};

fn checkpoint(sequence: u64) -> Checkpoint {
    Checkpoint {
        id: format!("checkpoint-{sequence}"),
        sequence,
        schema: 1,
        payload: vec![42; 4096],
    }
}

#[test]
fn flush_returns_committed_schema_boundary_and_preserves_checkpoints() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flush.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    assert_eq!(store.flush().unwrap(), store.status().unwrap());
    store.append(0, &[checkpoint(1)]).unwrap();
    store.migrate_schema(1, 2).unwrap();
    let boundary = store.flush().unwrap();
    assert_eq!(boundary.sequence, 1);
    assert_eq!(boundary.schema, 2);
    assert_eq!(boundary.schema_sequence, 1);
    assert_eq!(store.flush().unwrap(), boundary);
    drop(store);
    let reopened = CheckpointStore::open(&path).unwrap();
    assert_eq!(reopened.status().unwrap(), boundary);
    assert_eq!(reopened.load("checkpoint-1").unwrap(), Some(checkpoint(1)));
}

#[test]
fn busy_flush_is_not_a_success_proof_or_rollback_of_committed_data() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flush.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    store.append(0, &[checkpoint(1)]).unwrap();
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    assert_eq!(
        reader
            .query_row("SELECT highest_sequence FROM storage_meta", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        1
    );
    store.append(1, &[checkpoint(2)]).unwrap();
    assert!(matches!(store.flush(), Err(StorageError::Busy)));
    assert_eq!(store.sequence().unwrap(), 2);
    assert_eq!(store.load("checkpoint-2").unwrap(), Some(checkpoint(2)));
    reader.execute_batch("ROLLBACK").unwrap();
    assert_eq!(store.flush().unwrap().sequence, 2);
    drop(store);
    assert_eq!(CheckpointStore::open(&path).unwrap().sequence().unwrap(), 2);
}

#[test]
fn competing_writer_blocks_flush_without_claiming_its_uncommitted_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flush.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    store.append(0, &[checkpoint(1)]).unwrap();
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE storage_meta SET highest_sequence=2")
        .unwrap();
    assert!(matches!(store.flush(), Err(StorageError::Busy)));
    assert_eq!(store.sequence().unwrap(), 1);
    writer.execute_batch("ROLLBACK").unwrap();
    assert_eq!(store.flush().unwrap().sequence, 1);
}
