//! Real SQLite files and independent processes prove storage, not a memory model.

use std::{path::Path, process::Command};

use rusqlite::{params, Connection};

use crate::{AppendOutcome, Checkpoint, CheckpointStore, StorageError, MAX_CHECKPOINT_BYTES};

fn checkpoint(id: &str, sequence: u64) -> Checkpoint {
    Checkpoint {
        id: id.into(),
        sequence,
        schema: 1,
        payload: vec![sequence as u8; 8192],
    }
}

#[test]
fn committed_bytes_and_cas_survive_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let data = checkpoint("first", 1);
    let mut store = CheckpointStore::open(&path).unwrap();
    assert_eq!(store.sequence().unwrap(), 0);
    assert_eq!(
        store.append(0, std::slice::from_ref(&data)).unwrap(),
        AppendOutcome::Committed
    );
    drop(store);
    let mut reopened = CheckpointStore::open(&path).unwrap();
    assert_eq!(reopened.load("first").unwrap(), Some(data.clone()));
    assert_eq!(reopened.sequence().unwrap(), 1);
    assert_eq!(
        reopened.append(0, std::slice::from_ref(&data)).unwrap(),
        AppendOutcome::Replayed
    );
    let mut changed = data;
    changed.payload.push(1);
    assert!(matches!(
        reopened.append(0, &[changed]),
        Err(StorageError::Conflict)
    ));
    assert!(matches!(
        reopened.append(0, &[checkpoint("second", 2)]),
        Err(StorageError::Stale {
            expected: 0,
            actual: 1
        })
    ));
}

#[test]
fn independent_connections_serialize_cas_writers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let mut first = CheckpointStore::open(&path).unwrap();
    let mut second = CheckpointStore::open(&path).unwrap();
    first.append(0, &[checkpoint("first", 1)]).unwrap();
    assert!(matches!(
        second.append(0, &[checkpoint("loser", 1)]),
        Err(StorageError::Stale { .. })
    ));
    assert!(first.load("loser").unwrap().is_none());
    second.append(1, &[checkpoint("second", 2)]).unwrap();
    assert_eq!(first.sequence().unwrap(), 2);
}

#[test]
fn failed_batch_leaves_no_prefix_or_sequence_advance() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    let fault = Connection::open(&path).unwrap();
    fault.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON checkpoints WHEN NEW.id='second' BEGIN SELECT RAISE(ABORT,'injected transaction failure'); END;").unwrap();
    assert!(store
        .append(0, &[checkpoint("first", 1), checkpoint("second", 2)])
        .is_err());
    assert_eq!(store.sequence().unwrap(), 0);
    assert!(store.load("first").unwrap().is_none());
    fault.execute_batch("DROP TRIGGER reject_second").unwrap();
    store
        .append(0, &[checkpoint("first", 1), checkpoint("second", 2)])
        .unwrap();
    assert_eq!(store.sequence().unwrap(), 2);
}

#[test]
fn digest_binds_metadata_and_bytes_and_reports_corruption() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    store.append(0, &[checkpoint("first", 1)]).unwrap();
    let corruptor = Connection::open(&path).unwrap();
    corruptor
        .execute(
            "UPDATE checkpoints SET payload=?1 WHERE id='first'",
            [vec![0u8; 8192]],
        )
        .unwrap();
    let mut corrupted = checkpoint("first", 1);
    corrupted.payload = vec![0u8; 8192];
    assert!(
        matches!(store.load("first"), Err(StorageError::ChecksumMismatch { expected, actual })
        if expected == checkpoint("first", 1).checksum() && actual == corrupted.checksum())
    );
    assert!(matches!(
        store.append(0, &[checkpoint("first", 1)]),
        Err(StorageError::ChecksumMismatch { .. })
    ));
    assert_eq!(store.sequence().unwrap(), 1);
}

#[test]
fn sqlite_rejects_oversized_persisted_rows_before_the_rust_payload_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let store = CheckpointStore::open(&path).unwrap();
    let corruptor = Connection::open(&path).unwrap();
    corruptor
        .execute(
            "INSERT INTO checkpoints VALUES('oversized',1,1,zeroblob(?1),zeroblob(32))",
            [i64::try_from(MAX_CHECKPOINT_BYTES + 8192).unwrap()],
        )
        .unwrap();
    let error = store.load("oversized").unwrap_err();
    assert!(
        matches!(error, StorageError::Database(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::TooBig)
    );
}

#[test]
fn compaction_does_not_reuse_sequences_or_mutate_restored_views() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    let first = checkpoint("first", 1);
    store
        .append(0, &[first.clone(), checkpoint("second", 2)])
        .unwrap();
    let view = store.load("first").unwrap().unwrap();
    assert_eq!(
        store.compact(3).unwrap(),
        crate::CompactionOutcome {
            removed: 2,
            retained: 0,
            sequence: 2
        }
    );
    drop(store);
    let mut reopened = CheckpointStore::open(&path).unwrap();
    assert_eq!(view, first);
    assert_eq!(reopened.sequence().unwrap(), 2);
    assert!(reopened.load("first").unwrap().is_none());
    assert!(matches!(
        reopened.append(0, &[checkpoint("old", 1)]),
        Err(StorageError::Stale { .. })
    ));
    reopened.append(2, &[checkpoint("new", 3)]).unwrap();
}

#[test]
fn invalid_batches_do_not_mutate_durable_state() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CheckpointStore::open(&directory.path().join("state.sqlite")).unwrap();
    let mut oversized = checkpoint("large", 1);
    oversized.payload = vec![0; MAX_CHECKPOINT_BYTES + 1];
    for batch in [
        vec![],
        vec![checkpoint("zero", 0)],
        vec![checkpoint("duplicate", 1), checkpoint("duplicate", 2)],
        vec![checkpoint("late", 2), checkpoint("early", 1)],
        vec![oversized],
    ] {
        assert!(matches!(
            store.append(0, &batch),
            Err(StorageError::Invalid(_))
        ));
        assert_eq!(store.sequence().unwrap(), 0);
    }
}

#[test]
fn foreign_and_future_databases_are_rejected_without_relabelling() {
    let directory = tempfile::tempdir().unwrap();
    let foreign = directory.path().join("foreign.sqlite");
    let db = Connection::open(&foreign).unwrap();
    db.execute_batch("CREATE TABLE user_data(value TEXT); INSERT INTO user_data VALUES('keep');")
        .unwrap();
    assert!(matches!(
        CheckpointStore::open(&foreign),
        Err(StorageError::Incompatible)
    ));
    assert_eq!(
        db.query_row("SELECT value FROM user_data", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        db.pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let future = directory.path().join("future.sqlite");
    drop(CheckpointStore::open(&future).unwrap());
    let future_db = Connection::open(&future).unwrap();
    future_db
        .pragma_update(None, "user_version", i32::MAX)
        .unwrap();
    assert!(matches!(
        CheckpointStore::open(&future),
        Err(StorageError::Incompatible)
    ));
    assert_eq!(
        future_db
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        i64::from(i32::MAX)
    );
    assert!(matches!(
        CheckpointStore::open(Path::new("relative.sqlite")),
        Err(StorageError::Invalid(_))
    ));
}

#[test]
fn busy_writer_fails_explicitly_without_advancing_the_sequence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    let mut store = CheckpointStore::open(&path).unwrap();
    let competitor = Connection::open(&path).unwrap();
    competitor.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = store.append(0, &[checkpoint("blocked", 1)]).unwrap_err();
    assert!(
        matches!(error, StorageError::Database(rusqlite::Error::SqliteFailure(code, _))
        if code.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    assert_eq!(store.sequence().unwrap(), 0);
    competitor.execute_batch("ROLLBACK").unwrap();
    assert!(store.load("blocked").unwrap().is_none());
    store.append(0, &[checkpoint("accepted", 1)]).unwrap();
}

#[cfg(unix)]
#[test]
fn database_symlink_is_not_followed() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original.sqlite");
    let link = directory.path().join("link.sqlite");
    let store = CheckpointStore::open(&original).unwrap();
    std::os::unix::fs::symlink(&original, &link).unwrap();
    assert!(CheckpointStore::open(&link).is_err());
    assert_eq!(store.sequence().unwrap(), 0);
}

#[test]
fn committed_child_exit_recovers_and_uncommitted_child_exit_rolls_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.sqlite");
    drop(CheckpointStore::open(&path).unwrap());
    for mode in ["commit", "uncommitted"] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::storage_crash_child", "--nocapture"])
            .env("TERLAN_STORAGE_CRASH_TEST_PATH", &path)
            .env("TERLAN_STORAGE_CRASH_TEST_MODE", mode)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        let store = CheckpointStore::open(&path).unwrap();
        assert_eq!(store.sequence().unwrap(), 1);
        assert_eq!(
            store.load("committed").unwrap(),
            Some(checkpoint("committed", 1))
        );
        assert!(store.load("uncommitted").unwrap().is_none());
    }
}

#[test]
fn storage_crash_child() {
    let Some(path) = std::env::var_os("TERLAN_STORAGE_CRASH_TEST_PATH") else {
        return;
    };
    let mut store = CheckpointStore::open(Path::new(&path)).unwrap();
    if std::env::var("TERLAN_STORAGE_CRASH_TEST_MODE").unwrap() == "commit" {
        store.append(0, &[checkpoint("committed", 1)]).unwrap();
    } else {
        let connection = Connection::open(Path::new(&path)).unwrap();
        connection
            .execute_batch("PRAGMA cache_size=2; BEGIN IMMEDIATE;")
            .unwrap();
        connection
            .execute(
                "INSERT INTO checkpoints VALUES('uncommitted',2,1,?1,?2)",
                params![vec![42u8; 1024 * 1024], vec![0u8; 32]],
            )
            .unwrap();
        connection
            .execute("UPDATE storage_meta SET highest_sequence=2", [])
            .unwrap();
        // Deliberately bypass connection/transaction destructors in this child.
        std::process::exit(73);
    }
    // A successful commit must survive even without orderly connection close.
    std::process::exit(73);
}
