//! Volatile transactions, quotas, and immutable observations without filesystem authority.

use super::*;

fn checkpoint(id: &str, sequence: u64) -> Checkpoint {
    Checkpoint {
        id: id.into(),
        sequence,
        schema: 1,
        payload: vec![42],
    }
}

#[test]
fn local_limits_are_explicit_and_cannot_be_disabled() {
    for (bytes, count) in [
        (0, 1),
        (1, 0),
        (MAX_BATCH_BYTES + 1, 1),
        (1, MAX_BATCH_CHECKPOINTS + 1),
    ] {
        assert!(LocalCheckpointStore::new(bytes, count).is_err());
    }
}

#[test]
fn local_quota_failure_does_not_keep_a_batch_prefix_or_advance_state() {
    let mut store = LocalCheckpointStore::new(10, 4).unwrap();
    store.append(0, &[checkpoint("first", 1)]).unwrap();
    let before = store.status();
    assert!(store
        .append(1, &[checkpoint("next", 2), checkpoint("last", 3)])
        .is_err());
    assert_eq!(store.status(), before);
    assert_eq!(store.retained_bytes(), 6);
    assert!(store.load("next").is_none());
    assert!(store.load("last").is_none());
    assert_eq!(store.load("first"), Some(&checkpoint("first", 1)));
}

#[test]
fn local_capacity_counts_entries_and_compaction_reclaims_only_retained_bytes() {
    let mut store = LocalCheckpointStore::new(1024, 1).unwrap();
    store.append(0, &[checkpoint("one", 1)]).unwrap();
    let frozen = store.load("one").unwrap().clone();
    assert!(store.append(1, &[checkpoint("two", 2)]).is_err());
    assert!(store.compact(u64::MAX).is_err());
    let receipt = store.compact(2).unwrap();
    assert_eq!(
        receipt,
        CompactionOutcome {
            removed: 1,
            retained: 0,
            sequence: 1
        }
    );
    assert_eq!(store.retained_bytes(), 0);
    assert!(matches!(
        store.append(0, std::slice::from_ref(&frozen)),
        Err(StorageError::Stale { actual: 1, .. })
    ));
    assert_eq!(frozen, checkpoint("one", 1));
    store.append(1, &[checkpoint("two", 2)]).unwrap();
    assert_eq!(store.status().sequence, 2);
}

#[test]
fn local_cas_conflicts_invalid_batches_and_replay_share_durable_rules() {
    let mut store = LocalCheckpointStore::new(1024, 8).unwrap();
    let first = checkpoint("first", 1);
    assert_eq!(
        store.append(0, std::slice::from_ref(&first)).unwrap(),
        AppendOutcome::Committed
    );
    store.append(1, &[checkpoint("second", 2)]).unwrap();
    assert_eq!(
        store.append(0, std::slice::from_ref(&first)).unwrap(),
        AppendOutcome::Replayed
    );
    assert_eq!(store.status().sequence, 2);
    assert!(matches!(
        store.append(1, &[checkpoint("third", 3)]),
        Err(StorageError::Stale {
            expected: 1,
            actual: 2
        })
    ));
    assert!(matches!(
        store.append(0, &[checkpoint("first", 4)]),
        Err(StorageError::Conflict)
    ));
    assert!(matches!(
        store.append(0, &[first, checkpoint("third", 3)]),
        Err(StorageError::Conflict)
    ));
    assert!(store
        .append(2, &[checkpoint("third", 3), checkpoint("fourth", 3)])
        .is_err());
    assert!(store.load("third").is_none());
    assert_eq!(store.status().sequence, 2);
}

#[test]
fn local_schema_changes_preserve_old_checkpoints_without_rewriting_payloads() {
    let mut store = LocalCheckpointStore::new(1024, 8).unwrap();
    store.append(0, &[checkpoint("old", 1)]).unwrap();
    assert!(store.migrate_schema(0, 2).is_err());
    assert_eq!(
        store.migrate_schema(1, 2).unwrap(),
        StorageStatus {
            sequence: 1,
            schema: 2,
            schema_sequence: 1
        }
    );
    assert!(matches!(
        store.migrate_schema(1, 3),
        Err(StorageError::SchemaMismatch {
            expected: 1,
            actual: 2
        })
    ));
    assert!(matches!(
        store.append(1, &[checkpoint("new", 2)]),
        Err(StorageError::SchemaMismatch {
            expected: 1,
            actual: 2
        })
    ));
    let mut next = checkpoint("new", 2);
    next.schema = 2;
    store.append(1, &[next]).unwrap();
    assert_eq!(store.load("old").unwrap().schema, 1);
    assert_eq!(store.load("new").unwrap().schema, 2);
    assert_eq!(
        store.append(0, &[checkpoint("old", 1)]).unwrap(),
        AppendOutcome::Replayed
    );
}

#[test]
fn separate_local_stores_have_no_implicit_shared_or_persistent_state() {
    let mut first = LocalCheckpointStore::new(1024, 8).unwrap();
    first.append(0, &[checkpoint("private", 1)]).unwrap();
    let second = LocalCheckpointStore::new(1024, 8).unwrap();
    assert!(second.load("private").is_none());
    assert_eq!(second.status().sequence, 0);
    drop(first);
    let replacement = LocalCheckpointStore::new(1024, 8).unwrap();
    assert!(replacement.load("private").is_none());
    assert_eq!(replacement.status().schema, 1);
}
