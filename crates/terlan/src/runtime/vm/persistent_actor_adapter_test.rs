use super::super::{
    distributed_storage::{VmDistributedStorageMode, VmDistributedStoragePolicy},
    ReplValue,
};
use super::{
    cluster_persistent_actor_adapter_fixture, database_backed_persistent_actor_adapter_fixture,
    embedded_key_value_persistent_actor_adapter_fixture,
    execute_persistent_actor_adapter_cross_adapter_restore,
    file_backed_persistent_actor_adapter_fixture, local_persistent_actor_adapter_fixture,
    package_provided_persistent_actor_adapter_fixture, plan_persistent_actor_adapter_conformance,
    VmPersistentActorAdapterCheckpointFixture, VmPersistentActorAdapterConformanceError,
    VmPersistentActorAdapterEntryFixture,
};

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_local_fixture_replay() {
    let report = plan_persistent_actor_adapter_conformance(local_persistent_actor_adapter_fixture(
        "local-fixture",
    ))
    .expect("local adapter fixture should conform");

    assert_eq!(report.fixture_name, "local-fixture");
    assert_eq!(report.manifest.adapter_name, "force-local");
    assert_eq!(report.manifest.mode_kind, "local_only");
    assert_eq!(
        report.manifest.operations,
        vec![
            "open",
            "append",
            "flush",
            "compact",
            "load_snapshot",
            "close",
            "compare_and_swap_append",
            "snapshot_isolation",
            "durable_flush",
            "transactional_batch_append",
            "schema_migration",
            "resource_handle_validation"
        ]
    );
    assert!(!report.manifest.cluster_replication);
    assert_eq!(report.replayed_checkpoints.len(), 2);
    assert_eq!(report.replayed_checkpoints[0].checkpoint_id, "checkpoint-1");
    assert_eq!(report.replayed_checkpoints[1].sequence, 2);
    assert_eq!(report.retained_after_compaction, Some(1));
    assert_eq!(report.final_sequence, 2);
}

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_cluster_replication_fixture() {
    let report = plan_persistent_actor_adapter_conformance(
        cluster_persistent_actor_adapter_fixture("cluster-fixture"),
    )
    .expect("cluster adapter fixture should conform");

    assert_eq!(report.fixture_name, "cluster-fixture");
    assert_eq!(report.manifest.adapter_name, "cluster");
    assert_eq!(report.manifest.mode_kind, "cluster");
    assert!(report.manifest.cluster_replication);
    assert!(report.manifest.operations.contains(&"cluster_replicate"));
    assert_eq!(report.replayed_checkpoints.len(), 1);
    assert_eq!(
        report.replayed_checkpoints[0].checkpoint_id,
        "cluster-checkpoint-1"
    );
}

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay() {
    let report = plan_persistent_actor_adapter_conformance(
        file_backed_persistent_actor_adapter_fixture("file-backed-fixture"),
    )
    .expect("file-backed durable adapter fixture should conform");

    assert_eq!(report.fixture_name, "file-backed-fixture");
    assert_eq!(report.manifest.adapter_name, "file-backed");
    assert_eq!(report.manifest.mode_kind, "durable");
    assert!(!report.manifest.cluster_replication);
    assert!(report
        .manifest
        .operations
        .contains(&"compare_and_swap_append"));
    assert_eq!(report.replayed_checkpoints.len(), 2);
    assert_eq!(
        report.replayed_checkpoints[0].checkpoint_id,
        "file-checkpoint-1"
    );
    assert_eq!(report.replayed_checkpoints[1].sequence, 2);
    assert_eq!(report.retained_after_compaction, Some(1));
    assert_eq!(report.final_sequence, 2);
}

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay() {
    let report = plan_persistent_actor_adapter_conformance(
        database_backed_persistent_actor_adapter_fixture("database-backed-fixture"),
    )
    .expect("database-backed durable adapter fixture should conform");

    assert_eq!(report.fixture_name, "database-backed-fixture");
    assert_eq!(report.manifest.adapter_name, "database-backed");
    assert_eq!(report.manifest.mode_kind, "durable");
    assert!(!report.manifest.cluster_replication);
    assert!(report
        .manifest
        .operations
        .contains(&"compare_and_swap_append"));
    assert_eq!(report.replayed_checkpoints.len(), 3);
    assert_eq!(
        report.replayed_checkpoints[0].checkpoint_id,
        "database-checkpoint-1"
    );
    assert_eq!(
        report.replayed_checkpoints[2].checkpoint_id,
        "database-checkpoint-3"
    );
    assert_eq!(report.replayed_checkpoints[2].sequence, 3);
    assert_eq!(report.retained_after_compaction, Some(2));
    assert_eq!(report.final_sequence, 3);
}

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay() {
    let report = plan_persistent_actor_adapter_conformance(
        embedded_key_value_persistent_actor_adapter_fixture("embedded-key-value-fixture"),
    )
    .expect("embedded key/value durable adapter fixture should conform");

    assert_eq!(report.fixture_name, "embedded-key-value-fixture");
    assert_eq!(report.manifest.adapter_name, "embedded-key-value");
    assert_eq!(report.manifest.mode_kind, "durable");
    assert!(report
        .manifest
        .operations
        .contains(&"compare_and_swap_append"));
    assert_eq!(report.replayed_checkpoints.len(), 2);
    assert_eq!(
        report.replayed_checkpoints[0].checkpoint_id,
        "embedded-kv-checkpoint-1"
    );
    assert_eq!(report.replayed_checkpoints[1].sequence, 2);
    assert_eq!(report.retained_after_compaction, Some(1));
    assert_eq!(report.final_sequence, 2);
}

#[test]
fn vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay() {
    let report = plan_persistent_actor_adapter_conformance(
        package_provided_persistent_actor_adapter_fixture("package-provided-fixture"),
    )
    .expect("package-provided durable adapter fixture should conform");

    assert_eq!(report.fixture_name, "package-provided-fixture");
    assert_eq!(report.manifest.adapter_name, "package:example.audit-log");
    assert_eq!(report.manifest.mode_kind, "durable");
    assert_eq!(report.replayed_checkpoints.len(), 3);
    assert_eq!(
        report.replayed_checkpoints[2].checkpoint_id,
        "package-checkpoint-3"
    );
    assert_eq!(report.replayed_checkpoints[2].sequence, 3);
    assert_eq!(report.retained_after_compaction, Some(2));
    assert_eq!(report.final_sequence, 3);
}

#[test]
fn vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore() {
    let execution = execute_persistent_actor_adapter_cross_adapter_restore()
        .expect("cross-adapter restore should execute through adapter contract");

    assert_eq!(execution.source_adapter_kind, "embedded-key-value");
    assert_eq!(execution.destination_adapter_kind, "database-backed");
    assert_eq!(execution.snapshot_generation, 1);
    assert_eq!(execution.restored_event_count, 2);
    assert_eq!(execution.replayed_event_count, 2);
}

#[test]
fn vm_persistent_actor_adapter_conformance_rejects_missing_cluster_capability() {
    let mut fixture = local_persistent_actor_adapter_fixture("local-needs-cluster");
    fixture.require_cluster_replication = true;

    let error = plan_persistent_actor_adapter_conformance(fixture)
        .expect_err("local adapter must not satisfy cluster replication");

    assert_eq!(
        error,
        VmPersistentActorAdapterConformanceError::MissingClusterReplicationCapability {
            adapter_name: "force-local".to_string(),
            mode_kind: "local_only",
            outcome_kind: "unsupported",
            reason: "unsupported_operation",
        }
    );
}

#[test]
fn vm_persistent_actor_adapter_conformance_rejects_unavailable_adapter() {
    let mut fixture = local_persistent_actor_adapter_fixture("unavailable-durable");
    fixture.policy = VmDistributedStoragePolicy::new(
        "durable-offline",
        VmDistributedStorageMode::Durable,
        false,
    )
    .expect("durable policy");

    let error = plan_persistent_actor_adapter_conformance(fixture)
        .expect_err("unavailable adapter should be rejected");

    assert_eq!(
        error,
        VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
            step: "open",
            checkpoint_id: String::new(),
            outcome_kind: "storage_unavailable",
            reason: "storage_unavailable",
            recovery_action: "",
        }
    );
}

#[test]
fn vm_persistent_actor_adapter_conformance_rejects_corrupt_and_partial_checkpoints() {
    let mut corrupt = local_persistent_actor_adapter_fixture("corrupt");
    corrupt.checkpoints[0].corrupt_checksum = true;

    let corrupt_error = plan_persistent_actor_adapter_conformance(corrupt)
        .expect_err("corrupt checkpoint should be rejected");

    assert_eq!(
        corrupt_error,
        VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
            step: "compare_and_swap_append",
            checkpoint_id: "checkpoint-1".to_string(),
            outcome_kind: "checksum_mismatch",
            reason: "checksum_mismatch",
            recovery_action: "repair_snapshot",
        }
    );

    let mut partial = local_persistent_actor_adapter_fixture("partial");
    partial.checkpoints[0]
        .entries
        .push(VmPersistentActorAdapterEntryFixture {
            namespace: "state".to_string(),
            key: "profile".to_string(),
            value: ReplValue::Int(1),
        });
    partial.checkpoints[0].partial_write_limit = Some(1);

    let partial_error = plan_persistent_actor_adapter_conformance(partial)
        .expect_err("partial checkpoint should be rejected");

    assert_eq!(
        partial_error,
        VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
            step: "compare_and_swap_append",
            checkpoint_id: "checkpoint-1".to_string(),
            outcome_kind: "partial_write",
            reason: "partial_write",
            recovery_action: "rewrite_checkpoint",
        }
    );
}

#[test]
fn vm_persistent_actor_adapter_conformance_rejects_stale_replay_sequence() {
    let mut fixture = local_persistent_actor_adapter_fixture("stale");
    fixture.checkpoints = vec![checkpoint("checkpoint-5", 5), checkpoint("checkpoint-4", 4)];

    let error = plan_persistent_actor_adapter_conformance(fixture)
        .expect_err("stale checkpoint sequence should be rejected");

    assert_eq!(
        error,
        VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
            step: "compare_and_swap_append",
            checkpoint_id: "checkpoint-4".to_string(),
            outcome_kind: "stale_snapshot",
            reason: "stale_snapshot",
            recovery_action: "reject_replay",
        }
    );
}

#[test]
fn vm_persistent_actor_adapter_conformance_rejects_stale_compare_and_swap_token() {
    let mut fixture = local_persistent_actor_adapter_fixture("stale-cas-token");
    fixture.stale_compare_and_swap_token_for_test = true;

    let error = plan_persistent_actor_adapter_conformance(fixture)
        .expect_err("stale compare-and-swap token should be rejected");

    assert_eq!(
        error,
        VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
            step: "compare_and_swap_append",
            checkpoint_id: "checkpoint-2".to_string(),
            outcome_kind: "cas_token_mismatch",
            reason: "cas_token_mismatch",
            recovery_action: "reload_snapshot",
        }
    );
}

fn checkpoint(checkpoint_id: &str, sequence: u64) -> VmPersistentActorAdapterCheckpointFixture {
    VmPersistentActorAdapterCheckpointFixture {
        checkpoint_id: checkpoint_id.to_string(),
        sequence,
        entries: vec![VmPersistentActorAdapterEntryFixture {
            namespace: "state".to_string(),
            key: checkpoint_id.to_string(),
            value: ReplValue::Int(sequence as i64),
        }],
        corrupt_checksum: false,
        partial_write_limit: None,
    }
}
