#![allow(dead_code)]

use std::collections::BTreeSet;

use super::distributed_state::{
    VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
    VmDistributedStateVersion,
};
use super::distributed_storage::{
    VmDistributedStorageAdapter, VmDistributedStorageCasToken, VmDistributedStorageMode,
    VmDistributedStorageOperation, VmDistributedStorageOutcome, VmDistributedStoragePolicy,
    VmDistributedStorageSnapshot,
};
use super::persistent_actor_restore::{
    execute_persistent_actor_restore, VmPersistentActorExport,
    VmPersistentActorRestoreCapabilities, VmPersistentActorRestoreError,
    VmPersistentActorRestoreExecution, VmPersistentActorRestoreTarget,
};
use super::persistent_actor_store::{
    VmDatabaseBackedPersistentActorStore, VmPersistentActorEvent, VmPersistentActorId,
    VmPersistentActorSchema, VmPersistentActorSnapshot,
};
use super::ReplValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorAdapterCapabilityManifest {
    pub(crate) adapter_name: String,
    pub(crate) mode_kind: &'static str,
    pub(crate) operations: Vec<&'static str>,
    pub(crate) cluster_replication: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorAdapterEntryFixture {
    pub(crate) namespace: String,
    pub(crate) key: String,
    pub(crate) value: ReplValue,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorAdapterCheckpointFixture {
    pub(crate) checkpoint_id: String,
    pub(crate) sequence: u64,
    pub(crate) entries: Vec<VmPersistentActorAdapterEntryFixture>,
    pub(crate) corrupt_checksum: bool,
    pub(crate) partial_write_limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorAdapterConformanceFixture {
    pub(crate) name: String,
    pub(crate) policy: VmDistributedStoragePolicy,
    pub(crate) checkpoints: Vec<VmPersistentActorAdapterCheckpointFixture>,
    pub(crate) require_cluster_replication: bool,
    pub(crate) require_compare_and_swap: bool,
    pub(crate) stale_compare_and_swap_token_for_test: bool,
    pub(crate) compact_from_sequence: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorAdapterReplayResult {
    pub(crate) checkpoint_id: String,
    pub(crate) sequence: u64,
    pub(crate) checksum: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VmPersistentActorAdapterConformanceReport {
    pub(crate) fixture_name: String,
    pub(crate) manifest: VmPersistentActorAdapterCapabilityManifest,
    pub(crate) replayed_checkpoints: Vec<VmPersistentActorAdapterReplayResult>,
    pub(crate) retained_after_compaction: Option<usize>,
    pub(crate) final_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VmPersistentActorAdapterConformanceError {
    EmptyFixtureName,
    EmptyCheckpointSet,
    MissingClusterReplicationCapability {
        adapter_name: String,
        mode_kind: &'static str,
        outcome_kind: &'static str,
        reason: &'static str,
    },
    StorageOutcomeRejected {
        step: &'static str,
        checkpoint_id: String,
        outcome_kind: &'static str,
        reason: &'static str,
        recovery_action: &'static str,
    },
    ReplayDiverged {
        checkpoint_id: String,
        expected_sequence: u64,
        actual_sequence: u64,
    },
}

pub(crate) fn plan_persistent_actor_adapter_conformance(
    fixture: VmPersistentActorAdapterConformanceFixture,
) -> Result<VmPersistentActorAdapterConformanceReport, VmPersistentActorAdapterConformanceError> {
    if fixture.name.is_empty() {
        return Err(VmPersistentActorAdapterConformanceError::EmptyFixtureName);
    }
    if fixture.checkpoints.is_empty() {
        return Err(VmPersistentActorAdapterConformanceError::EmptyCheckpointSet);
    }

    let mut adapter = VmDistributedStorageAdapter::new(fixture.policy);
    let manifest = capability_manifest(&adapter);

    let opened = adapter.open();
    if opened.is_failure() {
        return Err(rejected("open", "", &opened));
    }

    if fixture.require_cluster_replication {
        let capability = adapter.require_cluster_replication();
        if capability.is_failure() {
            return Err(
                VmPersistentActorAdapterConformanceError::MissingClusterReplicationCapability {
                    adapter_name: manifest.adapter_name.clone(),
                    mode_kind: manifest.mode_kind,
                    outcome_kind: capability.kind(),
                    reason: capability.reason(),
                },
            );
        }
    }

    let mut replayed_checkpoints = Vec::new();
    for checkpoint in &fixture.checkpoints {
        if let Some(limit) = checkpoint.partial_write_limit {
            adapter.set_partial_write_limit_for_test(limit);
        }
        let snapshot = snapshot_from_fixture(checkpoint);
        let (append_step, append) = if fixture.require_cluster_replication {
            (
                "replicate_snapshot",
                adapter.replicate_snapshot(snapshot.clone()),
            )
        } else if fixture.require_compare_and_swap {
            let token = if fixture.stale_compare_and_swap_token_for_test {
                VmDistributedStorageCasToken::new(adapter.latest_sequence().saturating_sub(1))
            } else {
                adapter.compare_and_swap_token()
            };
            (
                "compare_and_swap_append",
                adapter.compare_and_swap_append(snapshot.clone(), token),
            )
        } else {
            ("append", adapter.append(snapshot.clone()))
        };
        if append.is_failure() {
            return Err(rejected(append_step, &checkpoint.checkpoint_id, &append));
        }

        let flushed = adapter.flush();
        if flushed.is_failure() {
            return Err(rejected("flush", &checkpoint.checkpoint_id, &flushed));
        }

        let loaded = adapter.load_snapshot(&checkpoint.checkpoint_id);
        let VmDistributedStorageOutcome::SnapshotLoaded(loaded_snapshot) = loaded else {
            return Err(rejected(
                "load_snapshot",
                &checkpoint.checkpoint_id,
                &loaded,
            ));
        };
        if loaded_snapshot.sequence != checkpoint.sequence {
            return Err(VmPersistentActorAdapterConformanceError::ReplayDiverged {
                checkpoint_id: checkpoint.checkpoint_id.clone(),
                expected_sequence: checkpoint.sequence,
                actual_sequence: loaded_snapshot.sequence,
            });
        }
        replayed_checkpoints.push(VmPersistentActorAdapterReplayResult {
            checkpoint_id: loaded_snapshot.checkpoint_id,
            sequence: loaded_snapshot.sequence,
            checksum: loaded_snapshot.checksum,
        });
    }

    let retained_after_compaction = fixture.compact_from_sequence.map(|retain_from_sequence| {
        let compacted = adapter.compact(retain_from_sequence);
        compacted.retained_snapshots()
    });
    let final_sequence = adapter.latest_sequence();
    let closed = adapter.close();
    if closed.is_failure() {
        return Err(rejected("close", "", &closed));
    }

    Ok(VmPersistentActorAdapterConformanceReport {
        fixture_name: fixture.name,
        manifest,
        replayed_checkpoints,
        retained_after_compaction,
        final_sequence,
    })
}

fn capability_manifest(
    adapter: &VmDistributedStorageAdapter,
) -> VmPersistentActorAdapterCapabilityManifest {
    let mut operations = vec![
        VmDistributedStorageOperation::Open.kind(),
        VmDistributedStorageOperation::Append.kind(),
        VmDistributedStorageOperation::Flush.kind(),
        VmDistributedStorageOperation::Compact.kind(),
        VmDistributedStorageOperation::LoadSnapshot.kind(),
        VmDistributedStorageOperation::Close.kind(),
        VmDistributedStorageOperation::CompareAndSwapAppend.kind(),
        VmDistributedStorageOperation::SnapshotIsolation.kind(),
        VmDistributedStorageOperation::DurableFlush.kind(),
        VmDistributedStorageOperation::TransactionalBatchAppend.kind(),
        VmDistributedStorageOperation::SchemaMigration.kind(),
        VmDistributedStorageOperation::ResourceHandleValidation.kind(),
    ];
    if adapter.can_cluster_replicate() {
        operations.push(VmDistributedStorageOperation::ClusterReplicate.kind());
    }
    VmPersistentActorAdapterCapabilityManifest {
        adapter_name: adapter.policy_name().to_string(),
        mode_kind: adapter.policy_mode_kind(),
        operations,
        cluster_replication: adapter.can_cluster_replicate(),
    }
}

fn snapshot_from_fixture(
    checkpoint: &VmPersistentActorAdapterCheckpointFixture,
) -> VmDistributedStorageSnapshot {
    let entries = checkpoint
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| VmDistributedStateEntry {
            scope: VmDistributedStateScope::new(&entry.namespace, &entry.key)
                .expect("test fixture scope"),
            owner_node_id: "persistent-actor-adapter-conformance".to_string(),
            value: entry.value.clone(),
            version: VmDistributedStateVersion::new(
                checkpoint.sequence + index as u64,
                "persistent-actor-adapter-conformance",
            )
            .expect("test fixture version"),
            policy: VmDistributedStatePolicy::LastWriterWins,
        })
        .collect::<Vec<_>>();
    let snapshot = VmDistributedStorageSnapshot::new(
        checkpoint.checkpoint_id.clone(),
        checkpoint.sequence,
        entries,
    )
    .expect("test fixture snapshot");
    if checkpoint.corrupt_checksum {
        VmDistributedStorageSnapshot::with_checksum(
            checkpoint.checkpoint_id.clone(),
            checkpoint.sequence,
            snapshot.entries,
            snapshot.checksum ^ 1,
        )
        .expect("corrupt test fixture snapshot")
    } else {
        snapshot
    }
}

fn rejected(
    step: &'static str,
    checkpoint_id: &str,
    outcome: &VmDistributedStorageOutcome,
) -> VmPersistentActorAdapterConformanceError {
    VmPersistentActorAdapterConformanceError::StorageOutcomeRejected {
        step,
        checkpoint_id: checkpoint_id.to_string(),
        outcome_kind: outcome.kind(),
        reason: outcome.reason(),
        recovery_action: outcome.recovery_action(),
    }
}

pub(crate) fn local_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::force_local(),
        checkpoints: vec![
            checkpoint("checkpoint-1", 1, "state", "cart", 1),
            checkpoint("checkpoint-2", 2, "state", "cart", 2),
        ],
        require_cluster_replication: false,
        require_compare_and_swap: true,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: Some(2),
    }
}

pub(crate) fn cluster_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::new("cluster", VmDistributedStorageMode::Cluster, true)
            .expect("cluster policy"),
        checkpoints: vec![checkpoint("cluster-checkpoint-1", 1, "state", "cart", 1)],
        require_cluster_replication: true,
        require_compare_and_swap: false,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: None,
    }
}

pub(crate) fn file_backed_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::new(
            "file-backed",
            VmDistributedStorageMode::Durable,
            true,
        )
        .expect("file-backed durable policy"),
        checkpoints: vec![
            checkpoint("file-checkpoint-1", 1, "state", "cart", 1),
            checkpoint("file-checkpoint-2", 2, "state", "cart", 2),
        ],
        require_cluster_replication: false,
        require_compare_and_swap: true,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: Some(2),
    }
}

pub(crate) fn database_backed_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::new(
            "database-backed",
            VmDistributedStorageMode::Durable,
            true,
        )
        .expect("database-backed durable policy"),
        checkpoints: vec![
            checkpoint("database-checkpoint-1", 1, "state", "order", 1),
            checkpoint("database-checkpoint-2", 2, "state", "order", 2),
            checkpoint("database-checkpoint-3", 3, "state", "order", 3),
        ],
        require_cluster_replication: false,
        require_compare_and_swap: true,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: Some(2),
    }
}

pub(crate) fn embedded_key_value_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::new(
            "embedded-key-value",
            VmDistributedStorageMode::Durable,
            true,
        )
        .expect("embedded key/value durable policy"),
        checkpoints: vec![
            checkpoint("embedded-kv-checkpoint-1", 1, "state", "session", 1),
            checkpoint("embedded-kv-checkpoint-2", 2, "state", "session", 2),
        ],
        require_cluster_replication: false,
        require_compare_and_swap: true,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: Some(2),
    }
}

pub(crate) fn package_provided_persistent_actor_adapter_fixture(
    name: impl Into<String>,
) -> VmPersistentActorAdapterConformanceFixture {
    VmPersistentActorAdapterConformanceFixture {
        name: name.into(),
        policy: VmDistributedStoragePolicy::new(
            "package:example.audit-log",
            VmDistributedStorageMode::Durable,
            true,
        )
        .expect("package-provided durable policy"),
        checkpoints: vec![
            checkpoint("package-checkpoint-1", 1, "state", "audit", 1),
            checkpoint("package-checkpoint-2", 2, "state", "audit", 2),
            checkpoint("package-checkpoint-3", 3, "state", "audit", 3),
        ],
        require_cluster_replication: false,
        require_compare_and_swap: true,
        stale_compare_and_swap_token_for_test: false,
        compact_from_sequence: Some(2),
    }
}

pub(crate) fn execute_persistent_actor_adapter_cross_adapter_restore(
) -> Result<VmPersistentActorRestoreExecution, VmPersistentActorRestoreError> {
    let actor_id = VmPersistentActorId::new("adapter-cross-actor").expect("actor id");
    let schema = VmPersistentActorSchema::new("adapter-cross-schema", 2).expect("schema");
    let snapshot = VmPersistentActorSnapshot::new(
        actor_id.clone(),
        schema.clone(),
        1,
        ReplValue::String("redacted-state".to_string()),
        Vec::new(),
        vec![50],
        vec!["db-session".to_string()],
        2,
    )
    .expect("snapshot");
    let export = VmPersistentActorExport::new(
        snapshot,
        vec![
            VmPersistentActorEvent::new(
                actor_id.clone(),
                schema.clone(),
                3,
                ReplValue::String("event-3".to_string()),
            )
            .expect("event"),
            VmPersistentActorEvent::new(
                actor_id.clone(),
                schema.clone(),
                4,
                ReplValue::String("event-4".to_string()),
            )
            .expect("event"),
        ],
        vec!["secret_token".to_string()],
        false,
    )?
    .with_source_adapter_kind("embedded-key-value");
    let target = VmPersistentActorRestoreTarget {
        actor_id,
        schema,
        available_resource_handles: BTreeSet::from(["db-session".to_string()]),
        capabilities: VmPersistentActorRestoreCapabilities::full(),
        adapter_kind: "database-backed".to_string(),
        allow_cross_adapter_restore: true,
        required_model_sync_streams: Vec::new(),
    };
    let mut destination =
        VmDatabaseBackedPersistentActorStore::new_database_backed("persistent_actor_restore")
            .expect("database-backed destination");
    execute_persistent_actor_restore(&export, &target, &mut destination)
}

fn checkpoint(
    checkpoint_id: impl Into<String>,
    sequence: u64,
    namespace: impl Into<String>,
    key: impl Into<String>,
    value: i64,
) -> VmPersistentActorAdapterCheckpointFixture {
    VmPersistentActorAdapterCheckpointFixture {
        checkpoint_id: checkpoint_id.into(),
        sequence,
        entries: vec![VmPersistentActorAdapterEntryFixture {
            namespace: namespace.into(),
            key: key.into(),
            value: ReplValue::Int(value),
        }],
        corrupt_checksum: false,
        partial_write_limit: None,
    }
}

#[cfg(test)]
#[path = "persistent_actor_adapter_test.rs"]
mod persistent_actor_adapter_test;
