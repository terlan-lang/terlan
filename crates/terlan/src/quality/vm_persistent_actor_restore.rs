use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-restore-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_STATE_RESTORE_ANCHORS: &[&str] = &[
    "export_snapshot",
    "import_snapshot",
    "snapshot contains duplicate state scope",
    "snapshot version must be valid",
    "BTreeMap<VmDistributedStateScope",
    "VmDistributedStateVersion",
];

const REQUIRED_STORAGE_RESTORE_ANCHORS: &[&str] = &[
    "VmDistributedStorageSnapshot",
    "load_snapshot",
    "SnapshotLoaded",
    "SnapshotMissing",
    "ChecksumMismatch",
    "expected_checksum",
    "checkpoint_id",
    "sequence",
];

const REQUIRED_INSPECTION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/timer.rs",
        &[
            "pub(crate) struct VmTimerSnapshot",
            "owner: VmProcessId",
            "deadline_tick: u64",
            "kind: VmTimerKind",
            "pub(crate) fn snapshots",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource.rs",
        &[
            "pub(crate) struct VmResourceSnapshot",
            "owner: VmProcessId",
            "kind: String",
            "label: String",
            "transfer_policy: VmResourceTransferPolicy",
            "pub(crate) fn snapshots",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/process.rs",
        &[
            "pub(crate) struct VmProcess",
            "mailbox: VecDeque<VmMessage>",
            "resource_handles: Vec<String>",
            "mailbox_len",
            "selective_receive",
        ],
    ),
];

const REQUIRED_RESTORE_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/vm/main.rs",
        &[
            "ExportPersistentActor",
            "RestorePersistentActor",
            "parse_export_persistent_actor_args",
            "parse_restore_persistent_actor_args",
            "render_persistent_actor_export_manifest",
            "render_persistent_actor_restore_plan",
            "export-persistent-actor",
            "restore-persistent-actor",
        ],
    ),
    (
        "crates/terlan/src/vm/main_test.rs",
        &[
            "parse_persistent_actor_export_command_accepts_manifest_metadata",
            "persistent_actor_export_command_renders_portable_manifest_without_payloads",
            "parse_persistent_actor_restore_command_accepts_validation_metadata",
            "persistent_actor_restore_command_renders_validated_plan_without_payloads",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_restore.rs",
        &[
            "VmPersistentActorExport",
            "VmPersistentActorRestoreTarget",
            "VmPersistentActorRestoreCapabilities",
            "plan_persistent_actor_restore",
            "CorruptExportChecksum",
            "WrongActorOwner",
            "StaleSchema",
            "MissingDurableResourceHandle",
            "ReorderedRetainedEventSuffix",
            "ReorderedMailboxCheckpoint",
            "IncompatibleAdapterKind",
            "IncompatibleAdapterForCompactedSnapshot",
            "VmPersistentActorCompactionRestore",
            "compaction_restore",
            "compacted_through_sequence",
            "VmPersistentActorReplayFixture",
            "VmPersistentActorRestoreExecution",
            "VmPersistentActorCrossMachineExport",
            "execute_persistent_actor_restore",
            "build_cross_machine_actor_export",
            "generate_minimal_actor_replay_fixture",
            "render_manifest",
            "validate_mailbox_checkpoint_order",
            "source_adapter_kind",
            "allow_cross_adapter_restore",
            "StoreRejected",
            "InvalidCrossMachineExportSource",
            "VmPersistentActorModelSyncContinuity",
            "MissingModelSyncContinuity",
            "ReorderedModelSyncStream",
            "model_sync_streams",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_restore_test.rs",
        &[
            "vm_persistent_actor_restore_accepts_deterministic_export_plan",
            "vm_persistent_actor_restore_rejects_corrupt_export_and_stale_schema",
            "vm_persistent_actor_restore_rejects_wrong_actor_and_missing_resource",
            "vm_persistent_actor_restore_rejects_reordered_event_suffix",
            "vm_persistent_actor_restore_rejects_reordered_mailbox_checkpoint",
            "vm_persistent_actor_restore_gates_compacted_snapshot_and_resource_adapter_support",
            "vm_persistent_actor_restore_rejects_incompatible_adapter_kind",
            "vm_persistent_actor_restore_executes_cross_adapter_restore",
            "vm_persistent_actor_restore_builds_cross_machine_export_format",
            "vm_persistent_actor_restore_accepts_compacted_export_with_restore_boundary",
            "vm_persistent_actor_restore_validates_model_sync_stream_continuity",
            "vm_persistent_actor_restore_rejects_missing_and_reordered_model_sync_stream",
            "vm_persistent_actor_restore_generates_minimal_replay_fixture_without_payloads",
            "pending_message",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_state_test.rs",
        &[
            "vm_distributed_state_exports_and_imports_deterministic_snapshots",
            "VmDistributedStateStore::import_snapshot(snapshot)",
            "restored.export_snapshot()",
            "snapshot contains duplicate state scope",
            "snapshot scope must be valid",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark",
            "vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark",
            "VmDistributedStorageOutcome::SnapshotLoaded",
            "repair_snapshot",
            "reject_replay",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/process_test.rs",
        &[
            "process_selective_receive_preserves_skipped_messages",
            "process_exit_clears_mailbox_and_returns_resource_handles",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer_test.rs",
        &[
            "timer_table_starts_one_shot_timer_and_exposes_snapshot",
            "timer_table_receive_timeout_wakes_blocked_process",
        ],
    ),
    (
        "std/vm/DistributedStateTest.terl",
        &[
            "checkpoint_restore_is_typed",
            "store.export_snapshot()",
            "DistributedState.restore(snapshot)",
        ],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "loaded = adapter.load_snapshot(\"checkpoint-b\")",
            "loaded_kind = DistributedStorage.kind(loaded)",
            "assert_equal(\"snapshot_loaded\", loaded_kind)",
        ],
    ),
    (
        "std/vm/PersistentActorTest.terl",
        &[
            "redaction_policy_is_source_visible",
            "PersistentActor.redaction_policy(",
            "RedactionPolicy",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-restore-check: vm-persistent-actor-compaction-check",
    "$(MAKE) vm-distributed-state-check",
    "$(MAKE) vm-timer-primitives-check",
    "$(MAKE) vm-resource-ownership-check",
    "vm_persistent_actor_restore_accepts_deterministic_export_plan",
    "vm_persistent_actor_restore_rejects_corrupt_export_and_stale_schema",
    "vm_persistent_actor_restore_rejects_wrong_actor_and_missing_resource",
    "vm_persistent_actor_restore_rejects_reordered_event_suffix",
    "vm_persistent_actor_restore_rejects_reordered_mailbox_checkpoint",
    "vm_persistent_actor_restore_gates_compacted_snapshot_and_resource_adapter_support",
    "vm_persistent_actor_restore_rejects_incompatible_adapter_kind",
    "vm_persistent_actor_restore_executes_cross_adapter_restore",
    "vm_persistent_actor_restore_builds_cross_machine_export_format",
    "parse_persistent_actor_export_command_accepts_manifest_metadata",
    "persistent_actor_export_command_renders_portable_manifest_without_payloads",
    "parse_persistent_actor_restore_command_accepts_validation_metadata",
    "persistent_actor_restore_command_renders_validated_plan_without_payloads",
    "vm_persistent_actor_restore_accepts_compacted_export_with_restore_boundary",
    "vm_persistent_actor_restore_validates_model_sync_stream_continuity",
    "vm_persistent_actor_restore_rejects_missing_and_reordered_model_sync_stream",
    "vm_persistent_actor_restore_generates_minimal_replay_fixture_without_payloads",
    "vm_persistent_actor_restore_test",
    "vm-persistent-actor-restore",
];

const EXPORT_MANIFESTS: &[&str] = &[
    "persistent actor export manifest: actor id, schema, generation, retained suffix, checksum",
    "distributed state export manifest: deterministic BTreeMap scope order",
    "storage checkpoint manifest: checkpoint id, sequence, checksum, entries",
    "timer inspection manifest: timer id, owner, deadline, kind",
    "resource inspection manifest: resource id, owner, kind, label, policy",
    "process inspection manifest: mailbox length and resource handle ownership",
    "cross-machine persistent actor export envelope: format, source machine, actor, schema, checksum",
    "public persistent actor export command: typed CLI metadata to portable manifest",
];

const REDACTION_DECISIONS: &[&str] = &[
    "raw storage adapter internals remain hidden from exported fixtures",
    "native resource handles expose VM ids and labels, not host pointers",
    "source-visible persistent actor redaction policy descriptor is typed",
    "mailbox payload redaction remains rejected until actor export API exists",
    "model-sync stream redaction remains rejected until stream export exists",
    "secret-aware export policy remains rejected until policy API exists",
];

const RESTORE_VALIDATION_TRACES: &[&str] = &[
    "state snapshot restore rejects duplicate scopes",
    "state snapshot restore rejects invalid scope/version metadata",
    "storage snapshot load rejects corrupt checksum",
    "storage stale replay returns typed reject_replay action",
    "timer snapshots carry owner and deadline before restore API exists",
    "resource snapshots carry owner and transfer policy before restore API exists",
    "persistent actor export checksum is verified before restore",
    "persistent actor restore rejects wrong actor owner",
    "persistent actor restore rejects stale schema",
    "persistent actor restore rejects missing durable resource handle",
    "persistent actor restore rejects reordered retained event suffix",
    "persistent actor restore rejects reordered mailbox checkpoint",
    "persistent actor restore rejects incompatible adapter kind",
    "persistent actor restore executes explicit cross-adapter store restore",
    "persistent actor restore rejects destination store conflict",
    "persistent actor cross-machine export rejects non-portable source machine id",
    "persistent actor restore accepts compacted export boundary metadata",
    "persistent actor restore validates model-sync stream continuity",
    "persistent actor restore rejects missing or reordered model-sync stream continuity",
    "persistent actor restore gates compacted snapshots on adapter capability",
    "persistent actor restore gates resource handles on adapter capability",
    "persistent actor restore generates payload-redacted replay fixture",
    "public persistent actor restore command validates restore plan",
];

const REJECTED_RESTORE_CASES: &[&str] = &[];

const MINIMAL_REPLAY_FIXTURES: &[&str] = &[
    "distributed state snapshot fixture",
    "distributed storage checkpoint fixture",
    "corrupt storage checkpoint fixture",
    "timer snapshot inspection fixture",
    "resource snapshot inspection fixture",
    "persistent actor payload-redacted replay fixture",
];

const CROSS_ADAPTER_RESTORE_RESULTS: &[&str] = &[
    "force-local load after reopen preserves checkpoint",
    "durable adapter load path exists but real backend restore is rejected",
    "cluster adapter replicated snapshot load path exists",
    "missing backend restore returns typed storage_unavailable",
    "actor export restored from embedded key/value source into database-backed store",
    "package-provided adapter restore remains rejected",
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm persistent actor restore summary.
pub struct VmPersistentActorRestoreSummary {
    pub export_manifest_count: usize,
    pub redaction_decision_count: usize,
    pub restore_validation_trace_count: usize,
    pub rejected_restore_case_count: usize,
    pub cross_adapter_restore_result_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm persistent actor restore.
pub fn run_vm_persistent_actor_restore(
    root: &Path,
) -> QualityResult<VmPersistentActorRestoreSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        REQUIRED_STATE_RESTORE_ANCHORS,
        "VM persistent actor restore state foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_STORAGE_RESTORE_ANCHORS,
        "VM persistent actor restore storage foundation",
    )?);
    for (relative, anchors) in REQUIRED_INSPECTION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor inspection foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_RESTORE_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor restore adversarial tests",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-persistent-actor-restore", &diagnostics));
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
        "schema": "terlan-vm-persistent-actor-restore-report-v1",
        "exportManifests": EXPORT_MANIFESTS,
        "redactionDecisions": REDACTION_DECISIONS,
        "restoreValidationTraces": RESTORE_VALIDATION_TRACES,
        "rejectedRestoreCases": REJECTED_RESTORE_CASES,
        "minimalReplayFixtures": MINIMAL_REPLAY_FIXTURES,
        "crossAdapterRestoreResults": CROSS_ADAPTER_RESTORE_RESULTS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM persistent actor restore report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorRestoreSummary {
        export_manifest_count: EXPORT_MANIFESTS.len(),
        redaction_decision_count: REDACTION_DECISIONS.len(),
        restore_validation_trace_count: RESTORE_VALIDATION_TRACES.len(),
        rejected_restore_case_count: REJECTED_RESTORE_CASES.len(),
        cross_adapter_restore_result_count: CROSS_ADAPTER_RESTORE_RESULTS.len(),
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
        .map_err(|err| format!("Makefile: failed to read persistent actor restore gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor restore gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("export manifests", EXPORT_MANIFESTS),
        ("redaction decisions", REDACTION_DECISIONS),
        ("restore validation traces", RESTORE_VALIDATION_TRACES),
        ("rejected restore cases", REJECTED_RESTORE_CASES),
        ("minimal replay fixtures", MINIMAL_REPLAY_FIXTURES),
        (
            "cross-adapter restore results",
            CROSS_ADAPTER_RESTORE_RESULTS,
        ),
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
                        "VM persistent actor restore {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_persistent_actor_restore_test.rs"]
#[cfg(test)]
mod vm_persistent_actor_restore_test;
