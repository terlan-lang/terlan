use super::*;

#[test]
pub(super) fn vm_distributed_storage_schema_migration_rejects_stale_expected_version() {
    let mut adapter = opened_force_local_adapter();
    assert_eq!(
        adapter.require_schema_migration(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.schema_migration_proof(),
        Ok(VmDistributedStorageSchemaMigrationProof::new(1, 0))
    );

    assert!(matches!(
        adapter.append(snapshot(
            "checkpoint-schema-a",
            1,
            vec![entry("state", "cart", 1)]
        )),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    let stale = adapter.migrate_schema(0, 2);
    assert_eq!(
        stale,
        VmDistributedStorageOutcome::SchemaMigrationMismatch {
            operation: VmDistributedStorageOperation::SchemaMigration,
            expected_schema: 0,
            actual_schema: 1,
        }
    );
    assert_eq!(stale.kind(), "schema_migration_mismatch");
    assert_eq!(stale.operation_kind(), "schema_migration");
    assert_eq!(stale.expected_schema(), 0);
    assert_eq!(stale.actual_schema(), 1);
    assert_eq!(stale.sequence(), 0);
    assert_eq!(stale.reason(), "schema_migration_mismatch");
    assert!(stale.is_failure());
    assert!(!stale.is_success());
    assert!(stale.requires_recovery());
    assert_eq!(stale.recovery_action(), "reload_schema");
    assert_eq!(
        adapter
            .schema_migration_proof()
            .expect("stale migration must not advance proof"),
        VmDistributedStorageSchemaMigrationProof::new(1, 0)
    );

    let migrated = adapter.migrate_schema(1, 2);
    assert_eq!(
        migrated,
        VmDistributedStorageOutcome::SchemaMigrated {
            schema_version: 2,
            sequence: 1,
        }
    );
    assert_eq!(migrated.kind(), "schema_migrated");
    assert_eq!(migrated.actual_schema(), 2);
    assert_eq!(migrated.sequence(), 1);
    assert_eq!(migrated.reason(), "");
    assert!(migrated.is_success());
    let proof = adapter
        .schema_migration_proof()
        .expect("successful schema migration proof");
    assert_eq!(proof.schema_version(), 2);
    assert_eq!(proof.sequence(), 1);

    let no_op = adapter.migrate_schema(2, 2);
    assert_eq!(
        no_op,
        VmDistributedStorageOutcome::SchemaMigrationMismatch {
            operation: VmDistributedStorageOperation::SchemaMigration,
            expected_schema: 2,
            actual_schema: 2,
        }
    );
    assert_eq!(
        adapter
            .schema_migration_proof()
            .expect("no-op migration must not advance proof"),
        VmDistributedStorageSchemaMigrationProof::new(2, 1)
    );

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.schema_migration_proof(),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::SchemaMigration,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_resource_handle_validation_rejects_missing_handles_without_mutation(
) {
    let mut adapter = opened_force_local_adapter();
    assert_eq!(
        adapter.require_resource_handle_validation(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.resource_handle_validation_proof(),
        Ok(VmDistributedStorageResourceHandleValidationProof::new(0, 0))
    );
    assert!(matches!(
        adapter.append(snapshot(
            "checkpoint-resource-a",
            1,
            vec![entry("state", "cart", 1)]
        )),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));

    let missing = adapter.validate_resource_handles(&["db.primary".to_string()]);
    assert_eq!(
        missing,
        VmDistributedStorageOutcome::ResourceHandleValidationFailed {
            operation: VmDistributedStorageOperation::ResourceHandleValidation,
            missing_handle: "db.primary".to_string(),
        }
    );
    assert_eq!(missing.kind(), "resource_handle_validation_failed");
    assert_eq!(missing.operation_kind(), "resource_handle_validation");
    assert_eq!(missing.missing_resource_handle(), "db.primary");
    assert_eq!(missing.validated_resource_count(), 0);
    assert_eq!(missing.reason(), "resource_handle_validation_failed");
    assert!(missing.is_failure());
    assert!(!missing.is_success());
    assert!(missing.requires_recovery());
    assert_eq!(missing.recovery_action(), "recover_resource_handle");
    assert_eq!(
        adapter
            .resource_handle_validation_proof()
            .expect("missing handle must not advance proof"),
        VmDistributedStorageResourceHandleValidationProof::new(0, 0)
    );

    let registered = adapter.register_resource_handle("db.primary");
    assert_eq!(
        registered,
        VmDistributedStorageOutcome::ResourceHandlesValidated {
            validated_count: 1,
            sequence: 1,
        }
    );
    assert_eq!(registered.kind(), "resource_handles_validated");
    assert_eq!(registered.validated_resource_count(), 1);
    assert_eq!(registered.sequence(), 1);

    let validated = adapter.validate_resource_handles(&["db.primary".to_string()]);
    assert_eq!(
        validated,
        VmDistributedStorageOutcome::ResourceHandlesValidated {
            validated_count: 1,
            sequence: 1,
        }
    );
    let proof = adapter
        .resource_handle_validation_proof()
        .expect("successful resource validation proof");
    assert_eq!(proof.validated_count(), 1);
    assert_eq!(proof.sequence(), 1);

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable unavailable policy");
    let unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert_eq!(
        unavailable_adapter.resource_handle_validation_proof(),
        Err(VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::ResourceHandleValidation,
            mode: VmDistributedStorageMode::Durable,
        })
    );
}

#[test]
pub(super) fn vm_distributed_storage_reports_flush_timeout_with_retry_recovery() {
    let mut adapter = opened_force_local_adapter();
    assert!(matches!(
        adapter.append(snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    adapter.timeout_next_flush_for_test();

    let timeout = adapter.flush();
    assert_eq!(
        timeout,
        VmDistributedStorageOutcome::FlushTimedOut {
            operation: VmDistributedStorageOperation::Flush,
            sequence: 1,
        }
    );
    assert_eq!(timeout.kind(), "flush_timed_out");
    assert_eq!(timeout.operation_kind(), "flush");
    assert_eq!(timeout.sequence(), 1);
    assert_eq!(timeout.checksum(), 0);
    assert_eq!(timeout.reason(), "flush_timed_out");
    assert!(timeout.is_failure());
    assert!(!timeout.is_success());
    assert!(timeout.requires_recovery());
    assert_eq!(timeout.recovery_action(), "retry_flush");
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
}

#[test]
pub(super) fn vm_distributed_storage_exposes_capability_operation_and_mode_kinds() {
    let mut local_adapter =
        VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert_eq!(local_adapter.policy_name(), "force-local");
    assert_eq!(local_adapter.policy_mode_kind(), "local_only");
    assert!(local_adapter.policy_available());
    assert!(!local_adapter.can_cluster_replicate());
    let opened = local_adapter.open();
    assert_eq!(opened.kind(), "opened");
    assert_eq!(opened.mode_kind(), "local_only");
    assert_eq!(opened.operation_kind(), "");

    let appended =
        local_adapter.append(snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)]));
    assert_eq!(appended.kind(), "appended");
    assert_eq!(appended.mode_kind(), "");
    assert_eq!(appended.operation_kind(), "");

    local_adapter.fail_next_flush_for_test();
    let finalize_failed = local_adapter.flush();
    assert_eq!(finalize_failed.kind(), "finalize_failed");
    assert_eq!(finalize_failed.operation_kind(), "flush");
    assert_eq!(finalize_failed.mode_kind(), "");

    let unsupported = local_adapter.require_cluster_replication();
    assert_eq!(unsupported.kind(), "unsupported");
    assert_eq!(unsupported.operation_kind(), "cluster_replicate");
    assert_eq!(unsupported.mode_kind(), "local_only");
    assert_eq!(unsupported.reason(), "unsupported_operation");
    assert!(unsupported.is_failure());
    assert!(!unsupported.is_success());
    assert!(!unsupported.requires_recovery());
    assert_eq!(unsupported.recovery_action(), "");

    let policy =
        VmDistributedStoragePolicy::new("durable-prod", VmDistributedStorageMode::Durable, false)
            .expect("policy should be valid");
    assert_eq!(policy.name(), "durable-prod");
    assert_eq!(policy.mode_kind(), "durable");
    assert!(!policy.is_available());
    assert!(!policy.can_cluster_replicate());
    let mut unavailable_adapter = VmDistributedStorageAdapter::new(policy);
    assert_eq!(unavailable_adapter.policy_name(), "durable-prod");
    assert_eq!(unavailable_adapter.policy_mode_kind(), "durable");
    assert!(!unavailable_adapter.policy_available());
    assert!(!unavailable_adapter.can_cluster_replicate());
    let unavailable = unavailable_adapter.append(snapshot(
        "checkpoint-unavailable",
        1,
        vec![entry("state", "cart", 1)],
    ));
    assert_eq!(unavailable.kind(), "storage_unavailable");
    assert_eq!(unavailable.operation_kind(), "append");
    assert_eq!(unavailable.mode_kind(), "durable");
    assert_eq!(unavailable.reason(), "storage_unavailable");
    assert!(unavailable.is_failure());
    assert!(!unavailable.is_success());
    assert!(!unavailable.requires_recovery());
    assert_eq!(unavailable.recovery_action(), "");
}

#[test]
pub(super) fn vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark() {
    let recovered = snapshot("checkpoint-restored", 7, vec![entry("state", "cart", 7)]);
    let mut adapter = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    adapter.inject_snapshot_for_test(recovered.clone());

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(adapter.latest_sequence(), 7);
    assert_eq!(
        adapter.load_snapshot("checkpoint-restored"),
        VmDistributedStorageOutcome::SnapshotLoaded(recovered)
    );
    assert_eq!(
        adapter.append(snapshot(
            "checkpoint-stale",
            6,
            vec![entry("state", "cart", 6)]
        )),
        VmDistributedStorageOutcome::StaleSnapshot {
            local_sequence: 7,
            incoming_sequence: 6,
        }
    );
    assert!(matches!(
        adapter.append(snapshot(
            "checkpoint-next",
            8,
            vec![entry("state", "cart", 8)]
        )),
        VmDistributedStorageOutcome::Appended { sequence: 8, .. }
    ));
}

#[test]
pub(super) fn vm_distributed_storage_durable_mode_writes_flushes_and_loads_snapshot() {
    let policy =
        VmDistributedStoragePolicy::new("durable-prod", VmDistributedStorageMode::Durable, true)
            .expect("durable policy should be valid");
    let mut adapter = VmDistributedStorageAdapter::new(policy);
    assert_eq!(adapter.policy_name(), "durable-prod");
    assert_eq!(adapter.policy_mode_kind(), "durable");
    assert!(adapter.policy_available());
    assert!(!adapter.can_cluster_replicate());

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Durable,
        }
    );
    let durable_snapshot = snapshot(
        "durable-checkpoint-a",
        1,
        vec![entry("state", "durable", 1)],
    );
    let checksum = durable_snapshot.checksum;
    assert_eq!(
        adapter.append(durable_snapshot.clone()),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "durable-checkpoint-a".to_string(),
            sequence: 1,
            checksum,
        }
    );
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
    assert_eq!(
        adapter.load_snapshot("durable-checkpoint-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(durable_snapshot)
    );
    let unsupported = adapter.replicate_snapshot(snapshot(
        "durable-replication",
        2,
        vec![entry("state", "durable", 2)],
    ));
    assert_eq!(
        unsupported,
        VmDistributedStorageOutcome::Unsupported {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(unsupported.operation_kind(), "cluster_replicate");
    assert_eq!(unsupported.mode_kind(), "durable");
    assert_eq!(unsupported.reason(), "unsupported_operation");
    assert!(!unsupported.requires_recovery());
}

#[test]
pub(super) fn vm_distributed_storage_cluster_capability_requires_cluster_mode_and_availability() {
    let available_policy =
        VmDistributedStoragePolicy::new("cluster-prod", VmDistributedStorageMode::Cluster, true)
            .expect("cluster policy should be valid");
    assert_eq!(available_policy.mode_kind(), "cluster");
    assert!(available_policy.is_available());
    assert!(available_policy.can_cluster_replicate());
    let available_adapter = VmDistributedStorageAdapter::new(available_policy);
    assert!(available_adapter.can_cluster_replicate());
    assert_eq!(
        available_adapter.require_cluster_replication(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Cluster,
        }
    );

    let unavailable_policy = VmDistributedStoragePolicy::new(
        "cluster-offline",
        VmDistributedStorageMode::Cluster,
        false,
    )
    .expect("cluster policy should be valid");
    assert_eq!(unavailable_policy.mode_kind(), "cluster");
    assert!(!unavailable_policy.is_available());
    assert!(!unavailable_policy.can_cluster_replicate());
    let mut unavailable_adapter = VmDistributedStorageAdapter::new(unavailable_policy);
    assert!(!unavailable_adapter.can_cluster_replicate());
    assert_eq!(
        unavailable_adapter.require_cluster_replication(),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::Cluster,
        }
    );
    let unavailable_replicate = unavailable_adapter.replicate_snapshot(snapshot(
        "cluster-offline-checkpoint",
        1,
        vec![entry("state", "offline", 1)],
    ));
    assert_eq!(
        unavailable_replicate,
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::Cluster,
        }
    );
    assert_eq!(unavailable_replicate.kind(), "storage_unavailable");
    assert_eq!(unavailable_replicate.operation_kind(), "cluster_replicate");
    assert_eq!(unavailable_replicate.mode_kind(), "cluster");
    assert_eq!(unavailable_replicate.reason(), "storage_unavailable");
    assert!(!unavailable_replicate.requires_recovery());
}

#[test]
pub(super) fn vm_distributed_storage_cluster_mode_writes_flushes_and_loads_snapshot() {
    let policy =
        VmDistributedStoragePolicy::new("cluster-prod", VmDistributedStorageMode::Cluster, true)
            .expect("cluster policy should be valid");
    let mut adapter = VmDistributedStorageAdapter::new(policy);
    assert_eq!(adapter.policy_name(), "cluster-prod");
    assert_eq!(adapter.policy_mode_kind(), "cluster");
    assert!(adapter.policy_available());
    assert!(adapter.can_cluster_replicate());

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Cluster,
        }
    );
    assert_eq!(
        adapter.require_cluster_replication(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Cluster,
        }
    );

    let snapshot = snapshot("cluster-checkpoint-a", 1, vec![entry("state", "cart", 1)]);
    let checksum = snapshot.checksum;
    assert_eq!(
        adapter.append(snapshot.clone()),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "cluster-checkpoint-a".to_string(),
            sequence: 1,
            checksum,
        }
    );
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
    assert_eq!(
        adapter.load_snapshot("cluster-checkpoint-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(snapshot)
    );
}

#[test]
pub(super) fn vm_distributed_storage_cluster_mode_replicates_snapshots_through_adapter() {
    let policy =
        VmDistributedStoragePolicy::new("cluster-prod", VmDistributedStorageMode::Cluster, true)
            .expect("cluster policy should be valid");
    let mut adapter = VmDistributedStorageAdapter::new(policy);
    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Cluster,
        }
    );

    let replicated = snapshot(
        "cluster-replicated-a",
        1,
        vec![entry("state", "replicated", 1)],
    );
    let checksum = replicated.checksum;
    assert_eq!(
        adapter.replicate_snapshot(replicated.clone()),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "cluster-replicated-a".to_string(),
            sequence: 1,
            checksum,
        }
    );
    assert_eq!(
        adapter.load_snapshot("cluster-replicated-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(replicated)
    );
}

#[test]
pub(super) fn vm_distributed_storage_cluster_replication_reports_typed_failures() {
    let mut local_adapter = opened_force_local_adapter();
    let local_rejection =
        local_adapter.replicate_snapshot(snapshot("local-replication", 1, vec![]));
    assert_eq!(
        local_rejection,
        VmDistributedStorageOutcome::Unsupported {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(local_rejection.operation_kind(), "cluster_replicate");
    assert_eq!(local_rejection.mode_kind(), "local_only");

    let cluster_policy =
        VmDistributedStoragePolicy::new("cluster-prod", VmDistributedStorageMode::Cluster, true)
            .expect("cluster policy should be valid");
    let mut closed_cluster = VmDistributedStorageAdapter::new(cluster_policy);
    let closed_rejection =
        closed_cluster.replicate_snapshot(snapshot("closed-replication", 1, vec![]));
    assert_eq!(
        closed_rejection,
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::Cluster,
        }
    );
    assert_eq!(closed_rejection.operation_kind(), "cluster_replicate");
    assert_eq!(closed_rejection.mode_kind(), "cluster");

    assert!(matches!(
        closed_cluster.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    let corrupt = VmDistributedStorageSnapshot::with_checksum(
        "cluster-corrupt",
        1,
        vec![entry("state", "replicated", 1)],
        1,
    )
    .expect("corrupt replicated snapshot descriptor should still build");
    let checksum_mismatch = closed_cluster.replicate_snapshot(corrupt.clone());
    assert_eq!(
        checksum_mismatch,
        VmDistributedStorageOutcome::ChecksumMismatch {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            checkpoint_id: "cluster-corrupt".to_string(),
            sequence: 1,
            expected: corrupt.expected_checksum(),
            actual: 1,
        }
    );
    assert_eq!(checksum_mismatch.operation_kind(), "cluster_replicate");
    assert_eq!(checksum_mismatch.recovery_action(), "repair_snapshot");

    closed_cluster.set_partial_write_limit_for_test(1);
    let partial = closed_cluster.replicate_snapshot(snapshot(
        "cluster-partial",
        2,
        vec![
            entry("state", "replicated-a", 1),
            entry("state", "replicated-b", 2),
        ],
    ));
    assert_eq!(
        partial,
        VmDistributedStorageOutcome::PartialWrite {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            checkpoint_id: "cluster-partial".to_string(),
            sequence: 2,
            expected_entries: 2,
            persisted_entries: 1,
        }
    );
    assert_eq!(partial.operation_kind(), "cluster_replicate");
    assert_eq!(partial.reason(), "partial_write");
    assert!(partial.requires_recovery());
    assert_eq!(partial.recovery_action(), "rewrite_checkpoint");
}

#[test]
pub(super) fn vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark() {
    let mut adapter = opened_force_local_adapter();
    let snapshot_a = snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)]);
    let snapshot_b = snapshot("checkpoint-b", 2, vec![entry("state", "cart", 2)]);

    assert!(matches!(
        adapter.append(snapshot_a.clone()),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::Flushed { sequence: 1 }
    );
    assert_eq!(adapter.close(), VmDistributedStorageOutcome::Closed);
    assert_eq!(
        adapter.load_snapshot("checkpoint-a"),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::LoadSnapshot,
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.load_snapshot("checkpoint-a"),
        VmDistributedStorageOutcome::SnapshotLoaded(snapshot_a)
    );
    assert_eq!(
        adapter.append(snapshot(
            "checkpoint-stale",
            1,
            vec![entry("state", "cart", 1)]
        )),
        VmDistributedStorageOutcome::StaleSnapshot {
            local_sequence: 1,
            incoming_sequence: 1,
        }
    );
    assert_eq!(
        adapter.append(snapshot_b.clone()),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "checkpoint-b".to_string(),
            sequence: 2,
            checksum: snapshot_b.checksum,
        }
    );
    assert_eq!(
        adapter.load_snapshot("checkpoint-b"),
        VmDistributedStorageOutcome::SnapshotLoaded(snapshot_b)
    );
}

#[test]
pub(super) fn vm_distributed_storage_closed_adapter_rejects_lifecycle_operations_until_reopen() {
    let mut adapter = opened_force_local_adapter();
    assert!(matches!(
        adapter.append(snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::Appended { sequence: 1, .. }
    ));
    assert_eq!(adapter.close(), VmDistributedStorageOutcome::Closed);

    for (operation, outcome) in [
        (
            VmDistributedStorageOperation::Append,
            adapter.append(snapshot(
                "checkpoint-closed",
                2,
                vec![entry("state", "cart", 2)],
            )),
        ),
        (VmDistributedStorageOperation::Flush, adapter.flush()),
        (VmDistributedStorageOperation::Compact, adapter.compact(1)),
        (
            VmDistributedStorageOperation::LoadSnapshot,
            adapter.load_snapshot("checkpoint-a"),
        ),
        (VmDistributedStorageOperation::Close, adapter.close()),
    ] {
        assert_eq!(
            outcome,
            VmDistributedStorageOutcome::StorageUnavailable {
                operation,
                mode: VmDistributedStorageMode::LocalOnly,
            }
        );
        assert_eq!(outcome.kind(), "storage_unavailable");
        assert_eq!(outcome.operation_kind(), operation.kind());
        assert_eq!(outcome.mode_kind(), "local_only");
        assert_eq!(outcome.reason(), "storage_unavailable");
        assert!(outcome.is_failure());
        assert!(!outcome.is_success());
        assert!(!outcome.requires_recovery());
    }

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(
        adapter.append(snapshot("checkpoint-b", 2, vec![entry("state", "cart", 2)])),
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "checkpoint-b".to_string(),
            sequence: 2,
            checksum: snapshot("checkpoint-b", 2, vec![entry("state", "cart", 2)]).checksum,
        }
    );
}

#[test]
pub(super) fn vm_distributed_storage_returns_unavailable_for_missing_backend_without_panics() {
    let policy =
        VmDistributedStoragePolicy::new("durable-prod", VmDistributedStorageMode::Durable, false)
            .expect("policy should be valid");
    let mut adapter = VmDistributedStorageAdapter::new(policy);

    assert_eq!(
        adapter.open(),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Open,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(
        adapter.append(snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)])),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Append,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(
        adapter.flush(),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Flush,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(
        adapter.compact(1),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Compact,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(
        adapter.load_snapshot("checkpoint-a"),
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::LoadSnapshot,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    let closed = adapter.close();
    assert_eq!(
        closed,
        VmDistributedStorageOutcome::StorageUnavailable {
            operation: VmDistributedStorageOperation::Close,
            mode: VmDistributedStorageMode::Durable,
        }
    );
    assert_eq!(closed.kind(), "storage_unavailable");
    assert_eq!(closed.operation_kind(), "close");
    assert_eq!(closed.mode_kind(), "durable");
    assert_eq!(closed.reason(), "storage_unavailable");
    assert!(!closed.requires_recovery());
}

#[test]
pub(super) fn vm_distributed_storage_reports_unsupported_cluster_replication_for_local_mode() {
    let adapter = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());

    let unsupported = adapter.require_cluster_replication();
    assert_eq!(
        unsupported,
        VmDistributedStorageOutcome::Unsupported {
            operation: VmDistributedStorageOperation::ClusterReplicate,
            mode: VmDistributedStorageMode::LocalOnly,
        }
    );
    assert_eq!(unsupported.kind(), "unsupported");
    assert_eq!(unsupported.sequence(), 0);
    assert_eq!(unsupported.checksum(), 0);
}

#[test]
pub(super) fn vm_distributed_storage_compare_and_swap_append_rejects_stale_token() {
    let mut adapter = opened_force_local_adapter();
    let initial_token = adapter.compare_and_swap_token();
    assert_eq!(initial_token.expected_sequence(), 0);

    let first = snapshot("checkpoint-a", 1, vec![entry("state", "cart", 1)]);
    let first_checksum = first.checksum;
    let appended_first = adapter.compare_and_swap_append(first, initial_token);
    assert_eq!(
        appended_first,
        VmDistributedStorageOutcome::Appended {
            checkpoint_id: "checkpoint-a".to_string(),
            sequence: 1,
            checksum: first_checksum,
        }
    );

    let stale_token = initial_token;
    let second = snapshot("checkpoint-b", 2, vec![entry("state", "cart", 2)]);
    let stale = adapter.compare_and_swap_append(second.clone(), stale_token);
    assert_eq!(
        stale,
        VmDistributedStorageOutcome::CompareAndSwapTokenMismatch {
            operation: VmDistributedStorageOperation::CompareAndSwapAppend,
            expected_sequence: 0,
            actual_sequence: 1,
        }
    );
    assert_eq!(stale.kind(), "cas_token_mismatch");
    assert_eq!(stale.operation_kind(), "compare_and_swap_append");
    assert_eq!(stale.reason(), "cas_token_mismatch");
    assert_eq!(stale.sequence(), 1);
    assert_eq!(stale.expected_sequence(), 0);
    assert_eq!(stale.actual_sequence(), 1);
    assert!(stale.is_failure());
    assert!(stale.requires_recovery());
    assert_eq!(stale.recovery_action(), "reload_snapshot");

    let fresh = adapter.compare_and_swap_token();
    assert_eq!(fresh.expected_sequence(), 1);
    assert!(matches!(
        adapter.compare_and_swap_append(second, fresh),
        VmDistributedStorageOutcome::Appended { sequence: 2, .. }
    ));
}

#[test]
pub(super) fn vm_distributed_storage_rejects_invalid_policy_and_snapshot_descriptors() {
    assert_eq!(
        VmDistributedStoragePolicy::new("", VmDistributedStorageMode::LocalOnly, true)
            .expect_err("empty policy name should fail"),
        "error[vm_distributed_storage]: storage policy name must be non-empty"
    );
    assert_eq!(
        VmDistributedStorageSnapshot::new("", 1, Vec::new())
            .expect_err("empty checkpoint id should fail"),
        "error[vm_distributed_storage]: checkpoint id must be non-empty"
    );
    assert_eq!(
        VmDistributedStorageSnapshot::new("checkpoint", 0, Vec::new())
            .expect_err("zero checkpoint sequence should fail"),
        "error[vm_distributed_storage]: checkpoint sequence must be non-zero"
    );
}

pub(super) fn opened_force_local_adapter() -> VmDistributedStorageAdapter {
    let mut adapter = VmDistributedStorageAdapter::new(VmDistributedStoragePolicy::force_local());
    assert!(matches!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened { .. }
    ));
    adapter
}

pub(super) fn opened_durable_adapter() -> VmDistributedStorageAdapter {
    let policy =
        VmDistributedStoragePolicy::new("durable-prod", VmDistributedStorageMode::Durable, true)
            .expect("durable policy should be valid");
    let mut adapter = VmDistributedStorageAdapter::new(policy);
    assert!(matches!(
        adapter.open(),
        VmDistributedStorageOutcome::Opened {
            mode: VmDistributedStorageMode::Durable,
        }
    ));
    adapter
}

pub(super) fn snapshot(
    checkpoint_id: &str,
    sequence: u64,
    entries: Vec<VmDistributedStateEntry>,
) -> VmDistributedStorageSnapshot {
    VmDistributedStorageSnapshot::new(checkpoint_id, sequence, entries)
        .expect("snapshot should be valid")
}

pub(super) fn entry(namespace: &str, key: &str, value: i64) -> VmDistributedStateEntry {
    VmDistributedStateEntry {
        scope: VmDistributedStateScope::new(namespace, key).expect("scope should be valid"),
        owner_node_id: "node-a".to_string(),
        value: ReplValue::Int(value),
        version: VmDistributedStateVersion::new(value as u64, "node-a")
            .expect("version should be valid"),
        policy: VmDistributedStatePolicy::WinnerTakesAll,
    }
}
