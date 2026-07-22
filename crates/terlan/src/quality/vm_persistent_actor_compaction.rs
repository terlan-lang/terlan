use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-compaction-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_STORAGE_COMPACTION_ANCHORS: &[&str] = &[
    "pub(crate) enum VmDistributedStorageOutcome",
    "Compacted",
    "SnapshotMissing",
    "retained_snapshots",
    "pub(crate) fn compact",
    "retain_from_sequence",
    "retain(|snapshot|",
    "pub(crate) fn latest_sequence",
    "StorageUnavailable",
    "requires_recovery",
    "VmDistributedStorageTransactionalRollbackProof",
    "last_batch_rollback_proof",
    "transactional_rollback_proof",
];

const REQUIRED_RESOURCE_CLEANUP_ANCHORS: &[&str] = &[
    "pub(crate) struct VmResourceSnapshot",
    "pub(crate) fn cleanup_owner",
    "VmResourceEvent::CleanedUpOnExit",
    "pub(crate) fn snapshots",
    "transfer_policy",
];

const REQUIRED_PROCESS_CLEANUP_ANCHORS: &[&str] = &[
    "mailbox: VecDeque<VmMessage>",
    "resource_handles: Vec<String>",
    "pub(crate) fn exit",
    "self.mailbox.clear",
    "self.resource_handles.drain",
    "exit_process",
];

const REQUIRED_STORAGE_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/persistent_actor_compaction.rs",
        &[
            "pub(crate) struct VmPersistentActorRetentionPolicy",
            "pub(crate) struct VmPersistentActorCompactionCandidate",
            "pub(crate) enum VmPersistentActorCompactionError",
            "pub(crate) fn plan_persistent_actor_compaction",
            "RetentionBeforeSchemaMigrationFloor",
            "RetentionBeforeAuditFloor",
            "CompactedSnapshotNotEquivalent",
            "RetainedEventGap",
            "ResourceHandlePrunedWithoutPolicy",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_compaction_test.rs",
        &[
            "vm_persistent_actor_compaction_accepts_equivalent_snapshot_and_suffix",
            "vm_persistent_actor_compaction_rejects_schema_and_audit_floor_loss",
            "vm_persistent_actor_compaction_rejects_unsafe_checkpoint_and_resource_pruning",
            "vm_persistent_actor_compaction_rejects_bad_retained_event_suffix",
            "vm_persistent_actor_compaction_rejects_non_equivalent_snapshot",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_compacts_old_snapshots_deterministically",
            "vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary",
            "vm_distributed_storage_compaction_preserves_monotonic_sequence_watermark",
            "vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary",
            "VmDistributedStorageOutcome::Compacted { retained: 2 }",
            "VmDistributedStorageOutcome::Compacted { retained: 0 }",
            "VmDistributedStorageOutcome::SnapshotMissing",
            "adapter.latest_sequence()",
            "vm_distributed_storage_returns_unavailable_for_missing_backend_without_panics",
            "VmDistributedStorageOperation::Compact",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/process_test.rs",
        &[
            "process_exit_clears_mailbox_and_returns_resource_handles",
            "process_resource_removal_cancellation_and_reduction_accounting_are_stable",
            "process.mailbox_len()",
            "process.resource_handles.is_empty()",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource_cancellation_test.rs",
        &["cancelled_process_resource_cleanup_makes_handles_stale"],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "compaction_and_checksum_metadata_are_typed",
            "adapter.compact(2)",
            "DistributedStorage.retained_snapshots(compacted)",
            "assert_equal(\"compacted\", DistributedStorage.kind(compacted))",
            "assert_equal(1, retained_snapshots)",
        ],
    ),
    (
        "std/vm/PersistentActorTest.terl",
        &[
            "retention_policy_is_source_visible",
            "PersistentActor.retention_policy(",
            "RetentionPolicy",
            "actor_family_retention_defaults_are_source_visible",
            "PersistentActor.family_retention_defaults(",
            "ActorFamilyRetentionDefaults",
            "audit_retention_plan_is_source_visible",
            "PersistentActor.audit_retention(",
            "AuditRetentionPlan",
            "package_retention_policy_binding_is_source_visible",
            "PersistentActor.package_retention_policy(",
            "PackageRetentionPolicyBinding",
            "model_sync_retention_continuity_is_source_visible",
            "PersistentActor.model_sync_retention_continuity(",
            "ModelSyncRetentionContinuityPlan",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-compaction-check: vm-persistent-actor-schema-check",
    "$(MAKE) vm-distributed-state-check",
    "$(MAKE) vm-resource-ownership-check",
    "runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_compaction_physically_removes_pruned_snapshots_and_retains_boundary",
    "runtime::vm::distributed_storage::distributed_storage_test::vm_distributed_storage_durable_transactional_batch_rollback_preserves_commit_boundary",
    "runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_accepts_equivalent_snapshot_and_suffix",
    "runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_schema_and_audit_floor_loss",
    "runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_unsafe_checkpoint_and_resource_pruning",
    "runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_bad_retained_event_suffix",
    "runtime::vm::persistent_actor_compaction::persistent_actor_compaction_test::vm_persistent_actor_compaction_rejects_non_equivalent_snapshot",
    "vm_persistent_actor_compaction_test",
    "vm-persistent-actor-compaction",
];

const BEFORE_AFTER_STORE_SIZES: &[&str] = &[
    "four checkpoints compacted with retain_from_sequence=3 retains two",
    "two checkpoints compacted with retain_from_sequence=3 retains zero",
    "closed adapter rejects compaction without mutating retained snapshots",
    "missing backend returns typed storage_unavailable for compaction",
    "retained snapshot count is surfaced through std.vm.DistributedStorage",
    "persistent actor candidate advances generation without changing schema",
];

const REPLAY_EQUIVALENCE_TRACES: &[&str] = &[
    "latest sequence watermark survives after compacting all stored snapshots",
    "replay at compacted sequence is rejected as stale",
    "next higher sequence appends after compaction",
    "retained checkpoint loads after compact/close/open",
    "compacted snapshot miss is typed and does not require recovery",
    "persistent actor compacted snapshot must equal replay final state",
];

const RETAINED_RANGES: &[&str] = &[
    "retain_from_sequence=3 keeps checkpoints 3..latest",
    "retain_from_sequence above latest keeps empty retained range",
    "retain_from_sequence=2 keeps checkpoint-b in std VM contract test",
    "resource snapshots remain live-only until actor compaction API exists",
    "mailbox checkpoint ranges remain rejected until persistent actor API exists",
    "retained event suffix rejects gaps and non-original sequences",
    "source-visible retention policy declares retain-from schema and audit floors",
    "source-visible actor-family retention defaults declare local and production policies",
    "source-visible audit retention plan declares required event evidence",
    "source-visible package retention policy binding declares package ownership",
    "source-visible model-sync retention continuity declares stream floor",
    "adapter physical compaction removes pruned snapshots and retains boundary",
];

const REJECTED_RETENTION_POLICIES: &[&str] = &[
    "schema-migration-aware retention planner",
    "mailbox checkpoint pruning API",
    "timer checkpoint pruning API",
    "durable resource garbage collection API",
    "retention before schema migration floor",
    "retention before audit floor",
];

const CRASH_INJECTION_CASES: &[&str] = &[
    "storage unavailable during compact returns typed failure",
    "partial event write remains handled by previous storage gate",
    "flush timeout remains handled by previous storage gate",
    "compaction after close is rejected before store mutation",
    "stale replay after compaction is rejected by sequence watermark",
    "non-equivalent compacted snapshot is rejected before adapter commit",
    "durable transactional batch rollback preserves pre-commit boundary",
];

const RESOURCE_CLEANUP_DECISIONS: &[&str] = &[
    "process exit clears mailbox before resource handle cleanup",
    "process exit returns owned resource handles in stable order",
    "resource table cleanup removes all resources for exiting owner",
    "stale resource handles are rejected after cleanup",
    "durable resource GC remains rejected until persistent actor API exists",
    "persistent actor resource handle pruning requires explicit policy",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPersistentActorCompactionSummary {
    pub before_after_store_size_count: usize,
    pub replay_equivalence_trace_count: usize,
    pub retained_range_count: usize,
    pub rejected_retention_policy_count: usize,
    pub crash_injection_case_count: usize,
    pub resource_cleanup_decision_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_persistent_actor_compaction(
    root: &Path,
) -> QualityResult<VmPersistentActorCompactionSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_STORAGE_COMPACTION_ANCHORS,
        "VM persistent actor compaction storage foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/resource.rs",
        REQUIRED_RESOURCE_CLEANUP_ANCHORS,
        "VM persistent actor compaction resource cleanup foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/process.rs",
        REQUIRED_PROCESS_CLEANUP_ANCHORS,
        "VM persistent actor compaction process cleanup foundation",
    )?);
    for (relative, anchors) in REQUIRED_STORAGE_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor compaction adversarial tests",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-persistent-actor-compaction",
            &diagnostics,
        ));
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
        "schema": "terlan-vm-persistent-actor-compaction-report-v1",
        "beforeAfterStoreSizes": BEFORE_AFTER_STORE_SIZES,
        "replayEquivalenceTraces": REPLAY_EQUIVALENCE_TRACES,
        "retainedRanges": RETAINED_RANGES,
        "rejectedRetentionPolicies": REJECTED_RETENTION_POLICIES,
        "crashInjectionCases": CRASH_INJECTION_CASES,
        "resourceCleanupDecisions": RESOURCE_CLEANUP_DECISIONS
    });
    let report_text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize VM persistent actor compaction report: {err}")
    })?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorCompactionSummary {
        before_after_store_size_count: BEFORE_AFTER_STORE_SIZES.len(),
        replay_equivalence_trace_count: REPLAY_EQUIVALENCE_TRACES.len(),
        retained_range_count: RETAINED_RANGES.len(),
        rejected_retention_policy_count: REJECTED_RETENTION_POLICIES.len(),
        crash_injection_case_count: CRASH_INJECTION_CASES.len(),
        resource_cleanup_decision_count: RESOURCE_CLEANUP_DECISIONS.len(),
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
    let text = fs::read_to_string(root.join("Makefile")).map_err(|err| {
        format!("Makefile: failed to read persistent actor compaction gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor compaction gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("before/after store sizes", BEFORE_AFTER_STORE_SIZES),
        ("replay equivalence traces", REPLAY_EQUIVALENCE_TRACES),
        ("retained ranges", RETAINED_RANGES),
        ("rejected retention policies", REJECTED_RETENTION_POLICIES),
        ("crash-injection cases", CRASH_INJECTION_CASES),
        ("resource cleanup decisions", RESOURCE_CLEANUP_DECISIONS),
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
                        "VM persistent actor compaction {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_persistent_actor_compaction_test.rs"]
mod vm_persistent_actor_compaction_test;
