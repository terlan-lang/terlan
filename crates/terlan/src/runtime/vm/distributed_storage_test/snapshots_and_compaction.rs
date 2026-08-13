use super::*;

use super::super::{
    VmDistributedStorageAdapter, VmDistributedStorageAtomicAppendProof,
    VmDistributedStorageDurableFlushProof, VmDistributedStorageMode, VmDistributedStorageOperation,
    VmDistributedStorageOutcome, VmDistributedStoragePolicy, VmDistributedStorageSnapshot,
    VmDistributedStorageSnapshotIsolationProof, VmDistributedStorageTransactionalBatchProof,
    VmDistributedStorageTransactionalRollbackProof,
};

#[test]
pub(super) fn vm_distributed_storage_force_local_writes_flushes_and_loads_snapshot() {
    let mut adapter = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert_eq!(adapter.policy_name(), "force-local");
    assert_eq!(adapter.policy_mode_kind(), "local_only");
    assert!(adapter.policy_available());
    assert!(!adapter.can_cluster_replicate());
    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    let snapshot = snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)]);
    let checksum = snapshot.checksum;

    let appended = adapter.append(snapshot.clone());
    assert_eq!(
        appended,
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "checkpoint-a".to_string(),
            sequence: 1,
            checksum,
        }
    );
    assert_eq!(appended.sequence(), 1);
    assert_eq!(appended.checkpoint_id(), "checkpoint-a");
    assert_eq!(appended.checksum(), checksum);
    assert_eq!(appended.reason(), "");
    assert!(!appended.is_failure());
    assert!(appended.is_success());
    assert!(!appended.requires_recovery());
    assert_eq!(appended.recovery_action(), "");
    let flushed = adapter.flush();
    assert_eq!(
        flushed,
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
    assert_eq!(flushed.kind(), "flushed");
    assert_eq!(flushed.sequence(), 1);
    assert_eq!(flushed.checksum(), 0);
    let loaded = adapter.load_snapshot("checkpoint-a");
    assert_eq!(
        loaded,
        VmDistributedStorageOutcome::SnapshotLoaded(snapshot)
    );
    assert_eq!(loaded.kind(), "snapshot_loaded");
    assert_eq!(loaded.sequence(), 1);
    assert_eq!(loaded.checkpoint_id(), "checkpoint-a");
    assert_eq!(loaded.checksum(), checksum);
    let closed = adapter.close();
    assert_eq!(closed, VmDistributedStorageOutcome::Closed);
    assert_eq!(closed.kind(), "closed");
    assert_eq!(closed.sequence(), 0);
    assert_eq!(closed.checkpoint_id(), "");
    assert_eq!(closed.checksum(), 0);
    assert_eq!(closed.reason(), "");
    assert!(!closed.is_failure());
    assert!(closed.is_success());
    assert!(!closed.requires_recovery());
    assert_eq!(closed.recovery_action(), "");
}

#[test]
pub(super) fn vm_distributed_storage_rejects_stale_snapshot_replay() {
    let mut adapter = opened_force_local_adapter();
    assert!(matches!(
        adapter.append(snapshot("checkpoint-a", 5, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::Appended { sequence: 5, .. }
    ));

    let stale = adapter.append(snapshot("checkpoint-b", 4, vec![entry("state", "cart", 2)]));
    assert_eq!(
        stale,
        VmDistributedStorageOutcome::StaleSnapshot {
            local_sequence: 5,
            incoming_sequence: 4,
        }
    );
    assert_eq!(stale.kind(), "stale_snapshot");
    assert_eq!(stale.sequence(), 4);
    assert_eq!(stale.local_sequence(), 5);
    assert_eq!(stale.incoming_sequence(), 4);
    assert_eq!(stale.checksum(), 0);
    assert_eq!(stale.reason(), "stale_snapshot");
    assert!(stale.is_failure());
    assert!(!stale.is_success());
    assert!(stale.requires_recovery());
    assert_eq!(stale.recovery_action(), "reject_replay");
    assert_eq!(adapter.latest_sequence(), 5);
}

#[test]
pub(super) fn vm_distributed_storage_compacts_old_snapshots_deterministically() {
    let mut adapter = opened_force_local_adapter();
    for sequence in 1..=4 {
        let id = format!("checkpoint-{sequence}");
        assert!(matches!(
            adapter.append(snapshot(
                &id,
                sequence,
                vec![entry("state", &id, sequence as i64)]
            )),
            VmDistributedStorageOutcome::Appended { .. }
        ));
    }

    let compacted = adapter.compact(3);
    assert_eq!(
        compacted,
        VmDistributedStorageOutcome::Compacted { retained: 2 }
    );
    assert_eq!(compacted.kind(), "compacted");
    assert_eq!(compacted.retained_snapshots(), 2);
    assert_eq!(compacted.reason(), "");
    assert!(!compacted.is_failure());
    assert!(compacted.is_success());
    assert!(!compacted.requires_recovery());
    assert_eq!(compacted.recovery_action(), "");
    let compacted_missing = adapter.load_snapshot("checkpoint-1");
    assert!(matches!(
        compacted_missing,
        VmDistributedStorageOutcome::SnapshotMissing {
            ref checkpoint_id,
        } if checkpoint_id == "checkpoint-1"
    ));
    assert_eq!(compacted_missing.kind(), "snapshot_missing");
    assert_eq!(compacted_missing.checkpoint_id(), "checkpoint-1");
    assert_eq!(compacted_missing.sequence(), 0);
    assert_eq!(compacted_missing.checksum(), 0);
    assert_eq!(compacted_missing.reason(), "snapshot_missing");
    assert!(matches!(
        adapter.load_snapshot("checkpoint-missing"),
        VmDistributedStorageOutcome::SnapshotMissing {
            checkpoint_id,
        } if checkpoint_id == "checkpoint-missing"
    ));
    assert!(matches!(
        adapter.close(),
        VmDistributedStorageOutcome::Closed
    ));
    let closed_load = adapter.load_snapshot("checkpoint-4");
    assert!(matches!(
        closed_load,
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::LoadSnapshot,
            mode: VmDistributedStorageMode::LocalOnly,
        }
    ));
    assert_eq!(closed_load.kind(), "storage_unavailable");
    assert!(matches!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    assert!(matches!(
        adapter.load_snapshot("checkpoint-4"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
}

#[test]
pub(super) fn vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary(
) {
    let mut adapter = opened_force_local_adapter();
    for sequence in 1..=4 {
        let id = format!("checkpoint-{sequence}");
        assert!(matches!(
            adapter.append(snapshot(
                &id,
                sequence,
                vec![entry("state", &id, sequence as i64)]
            )),
            VmDistributedStorageOutcome::Appended { .. }
        ));
    }

    let compacted = adapter.compact(3);
    assert_eq!(
        compacted,
        VmDistributedStorageOutcome::Compacted { retained: 2 }
    );
    assert!(matches!(
        adapter.load_snapshot("checkpoint-1"),
        VmDistributedStorageOutcome::SnapshotMissing { .. }
    ));
    assert!(matches!(
        adapter.load_snapshot("checkpoint-2"),
        VmDistributedStorageOutcome::SnapshotMissing { .. }
    ));
    assert!(matches!(
        adapter.load_snapshot("checkpoint-3"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
    assert!(matches!(
        adapter.load_snapshot("checkpoint-4"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
    assert_eq!(adapter.latest_sequence(), 4);
}

#[test]
pub(super) fn vm_distributed_storage_compaction_preserves_monotonic_sequence_watermark() {
    let mut adapter = opened_force_local_adapter();
    assert!(matches!(
        adapter.append(snapshot("checkpoint-1", 1, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::Appended { .. }
    ));
    assert!(matches!(
        adapter.append(snapshot("checkpoint-2", 2, vec![entry("state", "cart", 2)])),
        VmDistributedStorageOutcome::Appended { .. }
    ));

    let compacted = adapter.compact(3);
    assert_eq!(
        compacted,
        VmDistributedStorageOutcome::Compacted { retained: 0 }
    );
    assert_eq!(compacted.retained_snapshots(), 0);
    assert_eq!(adapter.latest_sequence(), 2);
    assert_eq!(
        adapter.append(snapshot(
            "checkpoint-replay",
            2,
            vec![entry("state", "cart", 2)]
        )),
        VmDistributedStorageOutcome::StaleSnapshot {
            local_sequence: 2,
            incoming_sequence: 2,
        }
    );
    assert!(matches!(
        adapter.append(snapshot("checkpoint-3", 3, vec![entry("state", "cart", 3)])),
        VmDistributedStorageOutcome::Appended { sequence: 3, .. }
    ));
}

#[test]
pub(super) fn vm_distributed_storage_detects_corrupt_snapshot_checksum() {
    let mut adapter = opened_force_local_adapter();
    let corrupt = VmDistributedStorageSnapshot::with_checksum(
        "checkpoint-a",
        1,
        vec![entry("state", "cart", 1)],
        1,
    )
    .expect("corrupt snapshot descriptor should still build");
    let checksum_mismatch = adapter.append(corrupt.clone());
    assert_eq!(
        checksum_mismatch,
        VmDistributedStorageOutcome::ChecksumMismatch {
            operation: VmDistributedStorageOperation::Append,
            checkpoint_id: "checkpoint-a".to_string(),
            sequence: 1,
            expected: corrupt.expected_checksum(),
            actual: 1,
        }
    );
    assert_eq!(checksum_mismatch.kind(), "checksum_mismatch");
    assert_eq!(checksum_mismatch.operation_kind(), "append");
    assert_eq!(checksum_mismatch.sequence(), 1);
    assert_eq!(checksum_mismatch.checksum(), 1);
    assert_eq!(
        checksum_mismatch.expected_checksum(),
        corrupt.expected_checksum()
    );
    assert_eq!(checksum_mismatch.reason(), "checksum_mismatch");
    assert!(checksum_mismatch.is_failure());
    assert!(!checksum_mismatch.is_success());
    assert!(checksum_mismatch.requires_recovery());
    assert_eq!(checksum_mismatch.recovery_action(), "repair_snapshot");
    assert_eq!(adapter.latest_sequence(), 0);

    adapter.inject_snapshot_for_test(corrupt.clone());

    let loaded_corrupt = adapter.load_snapshot("checkpoint-a");
    assert_eq!(
        loaded_corrupt,
        VmDistributedStorageOutcome::ChecksumMismatch {
            operation: VmDistributedStorageOperation::LoadSnapshot,
            checkpoint_id: "checkpoint-a".to_string(),
            sequence: 1,
            expected: corrupt.expected_checksum(),
            actual: 1,
        }
    );
    assert_eq!(loaded_corrupt.kind(), "checksum_mismatch");
    assert_eq!(loaded_corrupt.operation_kind(), "load_snapshot");
    assert_eq!(loaded_corrupt.checkpoint_id(), "checkpoint-a");
    assert_eq!(loaded_corrupt.sequence(), 1);
    assert_eq!(loaded_corrupt.checksum(), 1);
    assert_eq!(
        loaded_corrupt.expected_checksum(),
        corrupt.expected_checksum()
    );
    assert_eq!(loaded_corrupt.reason(), "checksum_mismatch");
    assert!(loaded_corrupt.is_failure());
    assert!(!loaded_corrupt.is_success());
    assert!(loaded_corrupt.requires_recovery());
    assert_eq!(loaded_corrupt.recovery_action(), "repair_snapshot");
}

#[test]
pub(super) fn vm_distributed_storage_reports_finalize_and_partial_write_failures() {
    let mut adapter = opened_force_local_adapter();
    let complete = snapshot(
        "checkpoint-complete",
        1,
        vec![entry("state", "cart", 1), entry("state", "profile", 1)],
    );
    let complete_checksum = complete.checksum;
    adapter.set_partial_write_limit_for_test(1);

    let partial = adapter.append(complete.clone());
    assert_eq!(
        partial,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::Append,
            checkpoint_id: "checkpoint-complete".to_string(),
            sequence: 1,
            expected_entries: 2,
            persisted_entries: 1,
        }
    );
    assert_eq!(partial.kind(), "partial_write");
    assert_eq!(partial.operation_kind(), "append");
    assert_eq!(partial.checkpoint_id(), "checkpoint-complete");
    assert_eq!(partial.sequence(), 1);
    assert_eq!(partial.checksum(), 0);
    assert_eq!(partial.expected_entries(), 2);
    assert_eq!(partial.persisted_entries(), 1);
    assert_eq!(partial.reason(), "partial_write");
    assert!(partial.is_failure());
    assert!(!partial.is_success());
    assert!(partial.requires_recovery());
    assert_eq!(partial.recovery_action(), "rewrite_checkpoint");
    assert_eq!(adapter.latest_sequence(), 0);
    assert!(matches!(
        adapter.load_snapshot("checkpoint-complete"),
        VmDistributedStorageOutcome::SnapshotMissing { .. }
    ));

    assert_eq!(
        adapter.append(complete),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "checkpoint-complete".to_string(),
            sequence: 1,
            checksum: complete_checksum,
        }
    );
    adapter.fail_next_flush_for_test();
    let failed_flush = adapter.flush();
    assert_eq!(
        failed_flush,
        VmDistributedStorageOutcome::FinalizeFailed {
            operation: VmDistributedStorageOperation::Flush,
            sequence: 1,
        }
    );
    assert_eq!(failed_flush.kind(), "finalize_failed");
    assert_eq!(failed_flush.sequence(), 1);
    assert_eq!(failed_flush.checksum(), 0);
    assert_eq!(failed_flush.expected_entries(), 0);
    assert_eq!(failed_flush.persisted_entries(), 0);
    assert_eq!(failed_flush.reason(), "finalize_failed");
    assert!(failed_flush.is_failure());
    assert!(!failed_flush.is_success());
    assert!(failed_flush.requires_recovery());
    assert_eq!(failed_flush.recovery_action(), "retry_finalize");
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
}

#[test]
pub(super) fn vm_distributed_storage_atomic_append_proof_preserves_sequence_on_failed_append() {
    let mut adapter = opened_force_local_adapter();
    assert_eq!(
        adapter.require_atomic_append(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.atomic_append_proof(),
        Ok(VmDistributedStorageAtomicAppendProof::new(0))
    );
    assert_eq!(
        adapter
            .atomic_append_proof()
            .expect("available atomic append proof")
            .observed_sequence(),
        0
    );

    let partial_snapshot = snapshot(
        "checkpoint-partial",
        1,
        vec![entry("state", "cart", 1), entry("state", "profile", 1)],
    );
    adapter.set_partial_write_limit_for_test(1);
    let partial = adapter.append(partial_snapshot);
    assert_eq!(
        partial,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::Append,
            checkpoint_id: "checkpoint-partial".to_string(),
            sequence: 1,
            expected_entries: 2,
            persisted_entries: 1,
        }
    );
    assert_eq!(adapter.latest_sequence(), 0);
    assert_eq!(
        adapter
            .atomic_append_proof()
            .expect("failed append must not advance proof")
            .observed_sequence(),
        0
    );

    let complete = snapshot("checkpoint-complete", 1, vec![entry("state", "cart", 1)]);
    assert!(matches!(
        adapter.append(complete),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    assert_eq!(
        adapter
            .atomic_append_proof()
            .expect("committed append advances proof")
            .observed_sequence(),
        1
    );

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.atomic_append_proof(),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Append,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_snapshot_isolation_proof_survives_later_compaction() {
    let mut adapter = opened_force_local_adapter();
    let snapshot_a = snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)]);
    let expected_checksum = snapshot_a.checksum;
    assert!(matches!(
        adapter.append(snapshot_a),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    assert_eq!(
        adapter.require_snapshot_isolation(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.snapshot_isolation_proof("checkpoint-a"),
        Ok(VmDistributedStorageSnapshotIsolationProof::new(
            "checkpoint-a",
            1,
            expected_checksum,
        ))
    );
    let proof = adapter
        .snapshot_isolation_proof("checkpoint-a")
        .expect("validated snapshot isolation proof");
    assert_eq!(proof.checkpoint_id(), "checkpoint-a");
    assert_eq!(proof.sequence(), 1);
    assert_eq!(proof.checksum(), expected_checksum);

    assert!(matches!(
        adapter.append(snapshot("checkpoint-b", 2, vec![entry("state", "cart", 2)])),
        VmDistributedStorageOutcome::Appended { sequence: 2, .. }
    ));
    assert_eq!(
        adapter.compact(2),
        VmDistributedStorageOutcome::Compacted { retained: 1 }
    );
    assert_eq!(
        adapter.load_snapshot("checkpoint-a"),
        VmDistributedStorageOutcome::SnapshotMissing {
            checkpoint_id: "checkpoint-a".to_string(),
        }
    );
    assert_eq!(proof.checkpoint_id(), "checkpoint-a");
    assert_eq!(proof.sequence(), 1);
    assert_eq!(proof.checksum(), expected_checksum);

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.snapshot_isolation_proof("checkpoint-a"),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::SnapshotIsolation,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_durable_flush_proof_advances_only_after_successful_flush() {
    let mut adapter = opened_force_local_adapter();
    assert_eq!(
        adapter.require_durable_flush(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.durable_flush_proof(),
        Ok(VmDistributedStorageDurableFlushProof::new(0))
    );
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("initial durable flush proof")
            .flushed_sequence(),
        0
    );

    assert!(matches!(
        adapter.append(snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    adapter.fail_next_flush_for_test();
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::FinalizeFailed {
            operation: VmDistributedStorageOperation::Flush,
            sequence: 1,
        }
    );
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("failed finalize must not advance durable proof")
            .flushed_sequence(),
        0
    );

    adapter.timeout_next_flush_for_test();
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::FlushTimedOut {
            operation: VmDistributedStorageOperation::Flush,
            sequence: 1,
        }
    );
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("timed out flush must not advance durable proof")
            .flushed_sequence(),
        0
    );

    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("successful flush advances durable proof")
            .flushed_sequence(),
        1
    );

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.durable_flush_proof(),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::DurableFlush,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_transactional_batch_rejects_partial_commit_without_mutation() {
    let mut adapter = opened_force_local_adapter();
    assert_eq!(
        adapter.require_transactional_batch(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.transactional_batch_proof(),
        Ok(VmDistributedStorageTransactionalBatchProof::new(0, 0, 0))
    );

    let first = snapshot("checkpoint-batch-a", 1, vec![entry("state", "cart", 1)]);
    let second = snapshot("checkpoint-batch-b", 2, vec![entry("state", "cart", 2)]);
    adapter.set_partial_write_limit_for_test(1);
    let partial = adapter.transactional_batch_append(vec![first.clone(), second.clone()]);
    assert_eq!(
        partial,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::TransactionalBatchAppend,
            checkpoint_id: "checkpoint-batch-a".to_string(),
            sequence: 2,
            expected_entries: 2,
            persisted_entries: 1,
        }
    );
    assert_eq!(partial.kind(), "partial_write");
    assert_eq!(partial.operation_kind(), "transactional_batch_append");
    assert_eq!(partial.sequence(), 2);
    assert_eq!(partial.expected_entries(), 2);
    assert_eq!(partial.persisted_entries(), 1);
    assert_eq!(partial.recovery_action(), "rewrite_checkpoint");
    assert_eq!(adapter.latest_sequence(), 0);
    assert_eq!(
        adapter.load_snapshot("checkpoint-batch-a"),
        VmDistributedStorageOutcome::SnapshotMissing {
            checkpoint_id: "checkpoint-batch-a".to_string(),
        }
    );
    assert_eq!(
        adapter
            .transactional_batch_proof()
            .expect("failed batch must not advance proof")
            .committed_count(),
        0
    );

    let committed = adapter.transactional_batch_append(vec![first, second]);
    assert!(matches!(
        committed,
        VmDistributedStorageOutcome::TransactionalBatchCommitted {
            first_sequence: 1,
            last_sequence: 2,
            count: 2,
            checksum,
        } if checksum > 0
    ));
    assert_eq!(committed.kind(), "batch_appended");
    assert_eq!(committed.sequence(), 2);
    assert_eq!(committed.first_sequence(), 1);
    assert_eq!(committed.last_sequence(), 2);
    assert_eq!(committed.committed_count(), 2);
    assert!(committed.checksum() > 0);
    assert_eq!(adapter.latest_sequence(), 2);
    assert!(matches!(
        adapter.load_snapshot("checkpoint-batch-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
    assert!(matches!(
        adapter.load_snapshot("checkpoint-batch-b"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
    let proof = adapter
        .transactional_batch_proof()
        .expect("committed batch proof");
    assert_eq!(proof.first_sequence(), 1);
    assert_eq!(proof.last_sequence(), 2);
    assert_eq!(proof.committed_count(), 2);

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.transactional_batch_proof(),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::TransactionalBatchAppend,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary(
) {
    let mut adapter = opened_durable_adapter();
    assert!(matches!(
        adapter.append(snapshot("durable-base", 1, vec![entry("state", "base", 1)])),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );

    let first = snapshot("durable-batch-a", 2, vec![entry("state", "batch-a", 2)]);
    let second = snapshot("durable-batch-b", 3, vec![entry("state", "batch-b", 3)]);
    adapter.set_partial_write_limit_for_test(1);
    let partial = adapter.transactional_batch_append(vec![first.clone(), second.clone()]);
    assert_eq!(
        partial,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::TransactionalBatchAppend,
            checkpoint_id: "durable-batch-a".to_string(),
            sequence: 3,
            expected_entries: 2,
            persisted_entries: 1,
        }
    );
    assert_eq!(partial.recovery_action(), "rewrite_checkpoint");
    assert_eq!(adapter.latest_sequence(), 1);
    assert_eq!(
        adapter
            .durable_flush_proof()
            .expect("rolled back batch must preserve durable flush proof")
            .flushed_sequence(),
        1
    );
    let rollback = adapter
        .transactional_rollback_proof()
        .expect("partial batch must record rollback proof");
    assert_eq!(
        rollback,
        VmDistributedStorageTransactionalRollbackProof::new(2, 1, 1)
    );
    assert_eq!(rollback.attempted_count(), 2);
    assert_eq!(rollback.persisted_entries(), 1);
    assert_eq!(rollback.restored_sequence(), 1);
    assert!(matches!(
        adapter.load_snapshot("durable-batch-a"),
        VmDistributedStorageOutcome::SnapshotMissing { .. }
    ));
    assert!(matches!(
        adapter.load_snapshot("durable-batch-b"),
        VmDistributedStorageOutcome::SnapshotMissing { .. }
    ));

    let committed = adapter.transactional_batch_append(vec![first, second]);
    assert!(matches!(
        committed,
        VmDistributedStorageOutcome::TransactionalBatchCommitted {
            first_sequence: 2,
            last_sequence: 3,
            count: 2,
            checksum,
        } if checksum > 0
    ));
    assert_eq!(adapter.latest_sequence(), 3);
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 3 }
    );
    assert!(matches!(
        adapter.load_snapshot("durable-batch-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
    assert!(matches!(
        adapter.load_snapshot("durable-batch-b"),
        VmDistributedStorageOutcome::SnapshotLoaded(_)
    ));
}
