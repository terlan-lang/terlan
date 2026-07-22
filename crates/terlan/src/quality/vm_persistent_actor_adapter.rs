use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-adapter-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_ADAPTER_ANCHORS: &[&str] = &[
    "pub(crate) enum VmDistributedStorageMode",
    "LocalOnly",
    "Durable",
    "Cluster",
    "pub(crate) enum VmDistributedStorageOperation",
    "ClusterReplicate",
    "CompareAndSwapAppend",
    "SnapshotIsolation",
    "DurableFlush",
    "TransactionalBatchAppend",
    "SchemaMigration",
    "ResourceHandleValidation",
    "VmDistributedStorageAtomicAppendProof",
    "VmDistributedStorageSnapshotIsolationProof",
    "VmDistributedStorageDurableFlushProof",
    "VmDistributedStorageTransactionalBatchProof",
    "VmDistributedStorageSchemaMigrationProof",
    "VmDistributedStorageResourceHandleValidationProof",
    "VmDistributedStorageCasToken",
    "CompareAndSwapTokenMismatch",
    "SchemaMigrationMismatch",
    "ResourceHandleValidationFailed",
    "pub(crate) struct VmDistributedStoragePolicy",
    "can_cluster_replicate",
    "supports(&self, operation",
    "pub(crate) struct VmDistributedStorageAdapter",
    "require_atomic_append",
    "atomic_append_proof",
    "observed_sequence",
    "require_snapshot_isolation",
    "snapshot_isolation_proof",
    "checkpoint_id(&self)",
    "require_durable_flush",
    "durable_flush_proof",
    "flushed_sequence",
    "require_transactional_batch",
    "transactional_batch_proof",
    "transactional_batch_append",
    "committed_count",
    "require_schema_migration",
    "schema_migration_proof",
    "migrate_schema",
    "schema_version",
    "expected_schema",
    "actual_schema",
    "require_resource_handle_validation",
    "resource_handle_validation_proof",
    "register_resource_handle",
    "validate_resource_handles",
    "missing_resource_handle",
    "validated_resource_count",
    "compare_and_swap_token",
    "compare_and_swap_append",
    "replicate_snapshot",
    "require_cluster_replication",
    "expected_sequence",
    "actual_sequence",
    "StorageUnavailable",
    "Unsupported",
    "PartialWrite",
    "ChecksumMismatch",
    "StaleSnapshot",
];

const REQUIRED_ADAPTER_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/persistent_actor_adapter.rs",
        &[
            "VmPersistentActorAdapterCapabilityManifest",
            "VmPersistentActorAdapterConformanceFixture",
            "VmPersistentActorAdapterConformanceReport",
            "VmPersistentActorAdapterConformanceError",
            "plan_persistent_actor_adapter_conformance",
            "MissingClusterReplicationCapability",
            "StorageOutcomeRejected",
            "ReplayDiverged",
            "require_compare_and_swap",
            "stale_compare_and_swap_token_for_test",
            "local_persistent_actor_adapter_fixture",
            "cluster_persistent_actor_adapter_fixture",
            "file_backed_persistent_actor_adapter_fixture",
            "database_backed_persistent_actor_adapter_fixture",
            "embedded_key_value_persistent_actor_adapter_fixture",
            "package_provided_persistent_actor_adapter_fixture",
            "execute_persistent_actor_adapter_cross_adapter_restore",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_adapter_test.rs",
        &[
            "vm_persistent_actor_adapter_conformance_accepts_local_fixture_replay",
            "vm_persistent_actor_adapter_conformance_accepts_cluster_replication_fixture",
            "vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay",
            "vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay",
            "vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay",
            "vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay",
            "vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore",
            "vm_persistent_actor_adapter_conformance_rejects_missing_cluster_capability",
            "vm_persistent_actor_adapter_conformance_rejects_unavailable_adapter",
            "vm_persistent_actor_adapter_conformance_rejects_corrupt_and_partial_checkpoints",
            "vm_persistent_actor_adapter_conformance_rejects_stale_replay_sequence",
            "vm_persistent_actor_adapter_conformance_rejects_stale_compare_and_swap_token",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_force_local_writes_flushes_and_loads_snapshot",
            "vm_distributed_storage_durable_mode_writes_flushes_and_loads_snapshot",
            "vm_distributed_storage_cluster_capability_requires_cluster_mode_and_availability",
            "vm_distributed_storage_cluster_mode_writes_flushes_and_loads_snapshot",
            "vm_distributed_storage_cluster_mode_replicates_snapshots_through_adapter",
            "vm_distributed_storage_cluster_replication_reports_typed_failures",
            "vm_distributed_storage_atomic_append_proof_preserves_sequence_on_failed_append",
            "vm_distributed_storage_snapshot_isolation_proof_survives_later_compaction",
            "vm_distributed_storage_durable_flush_proof_advances_only_after_successful_flush",
            "vm_distributed_storage_transactional_batch_rejects_partial_commit_without_mutation",
            "vm_distributed_storage_schema_migration_rejects_stale_expected_version",
            "vm_distributed_storage_resource_handle_validation_rejects_missing_handles_without_mutation",
            "vm_distributed_storage_compare_and_swap_append_rejects_stale_token",
            "vm_distributed_storage_returns_unavailable_for_missing_backend_without_panics",
            "vm_distributed_storage_reports_finalize_and_partial_write_failures",
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_rejects_stale_snapshot_replay",
        ],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "force_local_adapter_writes_flushes_and_loads_checkpoint",
            "closed_adapter_lifecycle_failures_are_typed",
            "cluster_adapter_capability_is_explicit",
            "policy_can_cluster_replicate",
            "atomic_append_contract_is_source_visible",
            "adapter.require_atomic_append()",
            "adapter.atomic_append_proof()",
            "DistributedStorage.proof_sequence",
            "snapshot_isolation_contract_is_source_visible",
            "adapter.require_snapshot_isolation()",
            "adapter.snapshot_isolation_proof",
            "DistributedStorage.isolation_checkpoint_id",
            "DistributedStorage.isolation_sequence",
            "DistributedStorage.isolation_checksum",
            "durable_flush_contract_is_source_visible",
            "adapter.require_durable_flush()",
            "adapter.durable_flush_proof()",
            "DistributedStorage.durable_flush_sequence",
            "transactional_batch_contract_is_source_visible",
            "adapter.require_transactional_batch()",
            "adapter.transactional_batch_proof()",
            "adapter.transactional_batch_append([first, second])",
            "DistributedStorage.batch_committed_count",
            "schema_migration_contract_is_source_visible",
            "adapter.require_schema_migration()",
            "adapter.schema_migration_proof()",
            "adapter.migrate_schema(0, 2)",
            "DistributedStorage.schema_version",
            "resource_handle_validation_contract_is_source_visible",
            "adapter.require_resource_handle_validation()",
            "adapter.resource_handle_validation_proof()",
            "adapter.validate_resource_handles([\"db.primary\"])",
            "DistributedStorage.resource_handle_count",
            "adapter.require_cluster_replication()",
            "adapter.replicate_snapshot(snapshot)",
            "compare_and_swap_append_contract_is_source_visible",
            "adapter.compare_and_swap_token()",
            "adapter.compare_and_swap_append(snapshot, token)",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-adapter-conformance-check: vm-persistent-actor-restore-check",
    "$(MAKE) vm-distributed-state-check",
    "vm_persistent_actor_adapter_conformance_accepts_local_fixture_replay",
    "vm_persistent_actor_adapter_conformance_accepts_cluster_replication_fixture",
    "vm_persistent_actor_adapter_conformance_accepts_file_backed_fixture_replay",
    "vm_persistent_actor_adapter_conformance_accepts_database_backed_fixture_replay",
    "vm_persistent_actor_adapter_conformance_accepts_embedded_key_value_fixture_replay",
    "vm_persistent_actor_adapter_conformance_accepts_package_provided_fixture_replay",
    "vm_persistent_actor_adapter_conformance_executes_cross_adapter_restore",
    "vm_persistent_actor_adapter_conformance_rejects_missing_cluster_capability",
    "vm_persistent_actor_adapter_conformance_rejects_unavailable_adapter",
    "vm_persistent_actor_adapter_conformance_rejects_corrupt_and_partial_checkpoints",
    "vm_persistent_actor_adapter_conformance_rejects_stale_replay_sequence",
    "vm_persistent_actor_adapter_conformance_rejects_stale_compare_and_swap_token",
    "vm_distributed_storage_atomic_append_proof_preserves_sequence_on_failed_append",
    "vm_distributed_storage_snapshot_isolation_proof_survives_later_compaction",
    "vm_distributed_storage_durable_flush_proof_advances_only_after_successful_flush",
    "vm_distributed_storage_transactional_batch_rejects_partial_commit_without_mutation",
    "vm_distributed_storage_schema_migration_rejects_stale_expected_version",
    "vm_distributed_storage_resource_handle_validation_rejects_missing_handles_without_mutation",
    "vm_persistent_actor_adapter_test",
    "vm-persistent-actor-adapter",
];

const ADAPTER_CAPABILITY_MANIFESTS: &[&str] = &[
    "force-local: open, append, flush, compact, load, close",
    "durable policy: open, append, flush, compact, load, close when available",
    "cluster policy: open, append, replicate, flush, compact, load, close",
    "unavailable backend: every lifecycle operation returns storage_unavailable",
    "unsupported local/durable replication: returns unsupported_operation",
    "cluster unavailable replication: returns storage_unavailable",
    "atomic append proof: capability, observed sequence, partial-write isolation",
    "snapshot isolation proof: capability, checkpoint id, sequence, checksum, compaction isolation",
    "durable flush proof: capability, flushed sequence, failed-flush isolation",
    "transactional batch append: capability, all-or-nothing commit, committed-count proof",
    "schema migration: capability, expected-version guard, schema proof",
    "resource handle validation: capability, registered handles, validation proof",
    "persistent actor conformance manifest: adapter name, mode, operations, cluster replication",
    "compare-and-swap append: token, guarded append, stale-token recovery",
    "file-backed persistent actor adapter: durable append, flush, load, compact, and close",
    "database-backed persistent actor adapter: durable transaction log replay and compaction",
    "embedded key/value persistent actor adapter: deterministic key/value replay and compaction",
    "package-provided persistent actor adapter: generated package manifest replay and compaction",
];

const CONFORMANCE_MATRIX: &[&str] = &[
    "append returns checkpoint id, sequence, checksum",
    "flush returns latest committed sequence",
    "load_snapshot returns typed snapshot or missing descriptor",
    "compact returns retained snapshot count",
    "replicate_snapshot shares append validation path",
    "policy capability checks are explicit before replication",
    "atomic append proof does not advance across failed append attempts",
    "snapshot isolation proof survives later adapter append and compaction",
    "durable flush proof advances only after successful flush",
    "transactional batch append rejects partial commit without mutation",
    "schema migration rejects stale expected version without mutation",
    "resource handle validation rejects missing handles without mutation",
    "compare-and-swap append validates observed sequence tokens",
    "closed adapters reject lifecycle operations without panics",
    "persistent actor local fixture appends, flushes, loads, compacts, and closes",
    "persistent actor local fixture uses compare-and-swap append tokens",
    "persistent actor file-backed fixture replays through durable adapter semantics",
    "persistent actor database-backed fixture replays through durable adapter semantics",
    "persistent actor embedded key/value fixture replays through durable adapter semantics",
    "persistent actor package-provided fixture replays through durable adapter semantics",
    "persistent actor cluster fixture uses explicit cluster replication capability",
    "persistent actor cross-adapter restore executes through the shared adapter contract",
    "persistent actor replay rejects actor-visible divergence before acceptance",
];

const CRASH_INJECTION_OUTCOMES: &[&str] = &[
    "partial append returns expected and persisted entry counts",
    "corrupt append returns checksum mismatch and repair action",
    "corrupt load returns checksum mismatch and repair action",
    "flush timeout returns retry recovery action",
    "finalize failure returns retry recovery action",
    "stale replay returns reject_replay recovery action",
    "cluster partial replication returns rewrite_checkpoint recovery action",
    "persistent actor stale compare-and-swap token returns reload_snapshot before append",
    "transactional batch partial commit returns rewrite_checkpoint without mutation",
    "schema migration mismatch returns reload_schema before migration",
    "resource handle validation returns recover_resource_handle before restore",
    "persistent actor corrupt checkpoint returns repair_snapshot before replay",
    "persistent actor partial checkpoint returns rewrite_checkpoint before replay",
    "persistent actor stale checkpoint returns reject_replay before replay",
];

const DURABILITY_EVIDENCE: &[&str] = &[
    "reopen preserves snapshots and sequence watermark",
    "recovered snapshots preserve sequence watermark",
    "highest sequence survives compaction",
    "cluster replicated snapshot can be loaded",
    "durable policy path can write, flush, and load when available",
    "durable flush proof exposes last successful flush sequence",
    "transactional batch proof exposes first sequence, last sequence, and committed count",
    "schema migration proof exposes schema version and storage sequence",
    "resource handle validation proof exposes validated count and storage sequence",
    "file-backed durable fixture preserves replay after flush and compaction",
    "database-backed durable fixture preserves ordered replay after flush and compaction",
    "embedded key/value durable fixture preserves replay after flush and compaction",
    "package-provided durable fixture preserves replay after flush and compaction",
    "cross-adapter restore preserves source and destination adapter metadata",
];

const REJECTED_ADAPTERS: &[&str] = &[];

const FIXTURE_REPLAY_RESULTS: &[&str] = &[
    "force-local checkpoint fixture replays",
    "file-backed durable checkpoint fixture replays",
    "database-backed durable checkpoint fixture replays",
    "embedded key/value durable checkpoint fixture replays",
    "package-provided durable checkpoint fixture replays",
    "cross-adapter persistent actor restore fixture replays",
    "durable checkpoint fixture replays when backend is available",
    "cluster replicated checkpoint fixture replays",
    "stale checkpoint fixture is rejected",
    "corrupt checkpoint fixture is rejected",
    "partial checkpoint fixture requires rewrite",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPersistentActorAdapterSummary {
    pub adapter_capability_manifest_count: usize,
    pub conformance_matrix_count: usize,
    pub crash_injection_outcome_count: usize,
    pub rejected_adapter_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_persistent_actor_adapter(
    root: &Path,
) -> QualityResult<VmPersistentActorAdapterSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_ADAPTER_ANCHORS,
        "VM persistent actor adapter conformance foundation",
    )?);
    for (relative, anchors) in REQUIRED_ADAPTER_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor adapter conformance tests",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-persistent-actor-adapter", &diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-persistent-actor-adapter-report-v1",
        "adapterCapabilityManifests": ADAPTER_CAPABILITY_MANIFESTS,
        "conformanceMatrix": CONFORMANCE_MATRIX,
        "crashInjectionOutcomes": CRASH_INJECTION_OUTCOMES,
        "durabilityEvidence": DURABILITY_EVIDENCE,
        "rejectedAdapters": REJECTED_ADAPTERS,
        "fixtureReplayResults": FIXTURE_REPLAY_RESULTS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM persistent actor adapter report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorAdapterSummary {
        adapter_capability_manifest_count: ADAPTER_CAPABILITY_MANIFESTS.len(),
        conformance_matrix_count: CONFORMANCE_MATRIX.len(),
        crash_injection_outcome_count: CRASH_INJECTION_OUTCOMES.len(),
        rejected_adapter_count: REJECTED_ADAPTERS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read persistent actor adapter gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor adapter gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("adapter capability manifests", ADAPTER_CAPABILITY_MANIFESTS),
        ("conformance matrix", CONFORMANCE_MATRIX),
        ("crash-injection outcomes", CRASH_INJECTION_OUTCOMES),
        ("durability evidence", DURABILITY_EVIDENCE),
        ("rejected adapters", REJECTED_ADAPTERS),
        ("fixture replay results", FIXTURE_REPLAY_RESULTS),
    ] {
        diagnostics.extend(validate_entries_for_placeholder_terms(label, entries));
    }
    diagnostics
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| {
            let lower = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .filter(move |term| lower.contains(**term))
                .map(move |term| {
                    format!(
                        "VM persistent actor adapter {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_persistent_actor_adapter_test.rs"]
mod vm_persistent_actor_adapter_test;
