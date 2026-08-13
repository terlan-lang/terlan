use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-store-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_PROCESS_ANCHORS: &[&str] = &[
    "pub(crate) struct VmProcessId",
    "pub(crate) enum VmProcessState",
    "pub(crate) struct VmProcessSource",
    "pub(crate) struct VmMessage",
    "pub(crate) struct VmProcess",
    "mailbox: VecDeque<VmMessage>",
    "mailbox_len",
    "receive_next",
    "selective_receive",
    "resource_handles",
    "exit_process",
];

const REQUIRED_TIMER_ANCHORS: &[&str] = &[
    "pub(crate) struct VmTimerId",
    "pub(crate) enum VmTimerKind",
    "pub(crate) enum VmTimerEvent",
    "pub(crate) struct VmTimerSnapshot",
    "pub(crate) struct VmTimerTable",
    "start_one_shot",
    "start_receive_timeout",
    "cancel_owner_timers",
    "advance_clock",
    "snapshots",
];

const REQUIRED_RESOURCE_ANCHORS: &[&str] = &[
    "pub(crate) struct VmResourceId",
    "pub(crate) enum VmResourceTransferPolicy",
    "pub(crate) struct VmResourceDescriptor",
    "pub(crate) struct VmResourceRecord",
    "pub(crate) struct VmResourceSnapshot",
    "pub(crate) enum VmResourceEvent",
    "cleanup_owner",
    "snapshots",
    "stale native resource handle",
];

const REQUIRED_STORAGE_ANCHORS: &[&str] = &[
    "pub(crate) struct VmDistributedStorageSnapshot",
    "pub(crate) enum VmDistributedStorageOutcome",
    "pub(crate) struct VmDistributedStorageAdapter",
    "append",
    "flush",
    "load_snapshot",
    "PartialWrite",
    "FlushTimedOut",
    "ChecksumMismatch",
    "StaleSnapshot",
    "requires_recovery",
    "recovery_action",
];

const REQUIRED_STATE_ANCHORS: &[&str] = &[
    "pub(crate) struct VmDistributedStateEntry",
    "pub(crate) struct VmDistributedStateVersion",
    "pub(crate) enum VmDistributedStateWriteOutcome",
    "export_snapshot",
    "import_snapshot",
    "Replayed",
    "Conflict",
];

const REQUIRED_MIGRATION_ANCHORS: &[&str] = &[
    "pub(crate) enum VmMigrationPhase",
    "pub(crate) struct VmMigrationIntent",
    "pub(crate) enum VmMigrationOutcome",
    "VmSchedulerEventKind",
    "MigrationPhaseAdvanced",
    "MigrationCommitted",
    "MigrationRolledBack",
    "completed_migration_outcomes",
    "commit_migration",
    "rollback_migration",
];

const REQUIRED_PERSISTENT_ACTOR_STORE_ANCHORS: &[&str] = &[
    "pub(crate) struct VmPersistentActorId",
    "pub(crate) struct VmPersistentActorSchema",
    "pub(crate) struct VmPersistentActorDeclaration",
    "pub(crate) struct VmPersistentActorSnapshot",
    "pub(crate) struct VmPersistentActorEvent",
    "pub(crate) struct VmPersistentActorReplay",
    "pub(crate) enum VmPersistentActorStoreOutcome",
    "pub(crate) trait VmPersistentActorStoreAdapter",
    "pub(crate) struct VmInMemoryPersistentActorStore",
    "pub(crate) struct VmFileBackedPersistentActorStore",
    "pub(crate) struct VmEmbeddedKeyValuePersistentActorStore",
    "pub(crate) struct VmDatabaseBackedPersistentActorStore",
    "store_snapshot",
    "append_event",
    "reject_partial_event",
    "events_after",
    "PartialWriteRejected",
    "IncompatibleSchema",
    "open_file_backed",
    "new_embedded_key_value",
    "from_embedded_key_values",
    "export_key_values",
    "new_database_backed",
    "from_database_rows",
    "export_database_rows",
    "database_backed_sql_statements",
    "persistent actor file-backed log is corrupt",
    "persistent actor embedded key/value store is corrupt",
    "persistent actor database-backed row is corrupt",
    "persistent actor storage lane must be non-empty",
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/process_test.rs",
        &[
            "process_table_sends_ordered_messages_and_wakes_recipient",
            "process_selective_receive_preserves_skipped_messages",
            "process_exit_clears_mailbox_and_returns_resource_handles",
            "process_selective_receive_preserves_large_skipped_mailbox_prefix",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer_test.rs",
        &[
            "timer_table_starts_one_shot_timer_and_exposes_snapshot",
            "timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order",
            "timer_table_receive_timeout_wakes_blocked_process",
            "timer_table_fires_equal_deadlines_in_timer_id_order",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource_cancellation_test.rs",
        &["cancelled_process_resource_cleanup_makes_handles_stale"],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_reports_finalize_and_partial_write_failures",
            "vm_distributed_storage_reports_flush_timeout_with_retry_recovery",
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark",
            "vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_scheduler/distributed_scheduler_test/migration.rs",
        &[
            "vm_distributed_scheduler_replays_duplicate_terminal_migration_outcomes",
            "vm_distributed_scheduler_rolls_back_storage_timeout_idempotently",
            "vm_distributed_scheduler_rolls_back_storage_partial_write_idempotently",
            "vm_distributed_scheduler_replays_duplicate_partial_commit_rollbacks_without_duplicate_envelopes",
        ],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "reopen_preserves_snapshots_and_sequence_watermark",
            "closed_adapter_lifecycle_failures_are_typed",
            "cluster_adapter_capability_is_explicit",
        ],
    ),
    (
        "std/vm/PersistentActorTest.terl",
        &[
            "typed_snapshot_schema_id_is_source_visible",
            "PersistentActor.schema_id",
            "PersistentActor.snapshot",
            "PersistentActor.replay",
            "PersistentActor.compatible_schema",
            "resource_restore_plan_is_source_visible",
            "PersistentActor.resource_checkpoint",
            "PersistentActor.restore_resource",
            "timer_restore_plan_is_source_visible",
            "PersistentActor.timer_checkpoint",
            "PersistentActor.restore_timer",
            "mailbox_restore_plan_is_source_visible",
            "PersistentActor.mailbox_checkpoint",
            "PersistentActor.restore_mailbox",
            "package_store_binding_is_source_visible",
            "PersistentActor.package_store",
            "persistent_actor_declaration_is_source_visible",
            "PersistentActor.persistent_actor",
            "PersistentActorDeclaration",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_store_test.rs",
        &[
            "vm_persistent_actor_store_replays_snapshot_and_events_deterministically",
            "vm_persistent_actor_store_rejects_stale_snapshot_and_schema_drift",
            "vm_persistent_actor_store_rejects_duplicate_and_partial_events_without_mutation",
            "vm_persistent_actor_store_restores_mailbox_timer_and_resource_checkpoints",
            "vm_persistent_actor_store_rejects_invalid_ids_schema_versions_and_handles",
            "vm_persistent_actor_declaration_binds_actor_schema_and_storage_lane",
            "vm_persistent_actor_declaration_rejects_invalid_storage_lanes",
            "vm_file_backed_persistent_actor_store_reopens_snapshot_and_events",
            "vm_file_backed_persistent_actor_store_rejects_corrupt_log",
            "vm_embedded_key_value_persistent_actor_store_exports_and_restores_snapshot_and_events",
            "vm_embedded_key_value_persistent_actor_store_rejects_corrupt_records",
            "vm_database_backed_persistent_actor_store_exports_sql_rows_and_replays",
            "vm_database_backed_persistent_actor_store_rejects_corrupt_rows_and_table_names",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-store-check: vm-model-sync-store-check",
    "$(MAKE) vm-process-model-check",
    "$(MAKE) vm-timer-primitives-check",
    "$(MAKE) vm-resource-ownership-check",
    "$(MAKE) vm-distributed-transport-check",
    "$(TERLC) check std/vm/PersistentActorTest.terl",
    "formal_pipeline::formal_pipeline_test::persistence_and_effect_interfaces::embedded_std_interfaces_include_vm_persistent_actor_contract",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_store_replays_snapshot_and_events_deterministically",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_declaration_binds_actor_schema_and_storage_lane",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_persistent_actor_declaration_rejects_invalid_storage_lanes",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_file_backed_persistent_actor_store_reopens_snapshot_and_events",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_file_backed_persistent_actor_store_rejects_corrupt_log",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_embedded_key_value_persistent_actor_store_exports_and_restores_snapshot_and_events",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_embedded_key_value_persistent_actor_store_rejects_corrupt_records",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_database_backed_persistent_actor_store_exports_sql_rows_and_replays",
    "runtime::vm::persistent_actor_store::persistent_actor_store_test::vm_database_backed_persistent_actor_store_rejects_corrupt_rows_and_table_names",
    "vm_persistent_actor_store_test",
    "vm-persistent-actor-store",
];

const ADAPTER_MATRIX: &[&str] = &[
    "force-local storage adapter: deterministic in-memory snapshots",
    "in-memory persistent actor adapter: snapshot, event log, and replay contract",
    "durable storage mode: typed durable policy without public adapter API",
    "cluster storage mode: replication-capable adapter contract foundation",
    "source-visible persistent actor declaration: typed actor/schema storage lane",
    "file-backed persistent actor adapter: deterministic typed file log replay",
    "embedded key/value persistent actor adapter: deterministic VM-owned keyspace replay",
    "database-backed persistent actor adapter: deterministic SQL row replay",
    "package-provided store adapter: source-visible package store binding",
];

const SNAPSHOT_EVENT_FIXTURES: &[&str] = &[
    "process mailbox fixture: ordered messages and selective receive cursor",
    "distributed state snapshot fixture: deterministic scope order",
    "distributed storage checkpoint fixture: checksum, sequence, entries",
    "persistent actor store fixture: snapshot, event log, mailbox, timers, resources",
    "file-backed persistent actor fixture: reopen typed log and reject corrupt records",
    "embedded key/value persistent actor fixture: export typed keyspace and reject corrupt records",
    "database-backed persistent actor fixture: export SQL rows and reject corrupt records",
    "source-visible persistent actor schema fixture: typed snapshot schema id",
    "source-visible persistent actor declaration fixture: actor id, schema, storage lane",
    "migration event fixture: requested, snapshotting, transfer, resume, terminal",
    "resource handle fixture: VM-owned durable handle identity and cleanup",
];

const REPLAY_TRACES: &[&str] = &[
    "identical distributed-state write replays without mutation",
    "duplicate terminal migration commit replays without duplicate events",
    "duplicate timeout rollback replays without duplicate failure envelopes",
    "duplicate partial-write rollback replays without duplicate envelopes",
    "persistent actor replay restores snapshot and sorted event suffix",
    "snapshot replay is rejected when stale",
];

const SCHEMA_MIGRATION_CASES: &[&str] = &[
    "snapshot sequence is validated before restore",
    "snapshot checksum is validated before load",
    "state snapshot rejects invalid scopes and versions",
    "schema-id migration graph remains rejected until Slice 123",
];

const MAILBOX_TIMER_RECOVERY: &[&str] = &[
    "mailbox preserves FIFO order for ordinary receive",
    "selective receive preserves skipped messages",
    "large skipped mailbox prefix remains stable",
    "source-visible mailbox restore API fixture: typed mailbox checkpoint and restore plan",
    "timer snapshots expose owner, deadline, and kind",
    "receive timeout wakes blocked process",
    "owner timer cancellation is stable after process exit",
    "source-visible timer restore API fixture: typed timer checkpoint and restore plan",
];

const RESOURCE_HANDLE_VALIDATION: &[&str] = &[
    "resource snapshots expose owner, kind, label, transfer policy",
    "process exit returns owned handles for cleanup",
    "cancelled process cleanup makes handles stale",
    "resource transfer validates live source and target owners",
    "source-visible resource restore API fixture: typed durable resource checkpoint and restore plan",
];

const CRASH_INJECTION_OUTCOMES: &[&str] = &[
    "partial snapshot write requires checkpoint rewrite",
    "flush timeout requires retry without advancing committed state",
    "checksum mismatch requires snapshot repair",
    "stale checkpoint replay is rejected",
    "migration storage failures roll back idempotently",
];

const REJECTED_PERSISTENT_ACTOR_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm persistent actor store summary.
pub struct VmPersistentActorStoreSummary {
    pub adapter_matrix_count: usize,
    pub snapshot_event_fixture_count: usize,
    pub replay_trace_count: usize,
    pub rejected_persistent_actor_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm persistent actor store.
pub fn run_vm_persistent_actor_store(root: &Path) -> QualityResult<VmPersistentActorStoreSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/process.rs",
        REQUIRED_PROCESS_ANCHORS,
        "VM persistent actor process foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/timer.rs",
        REQUIRED_TIMER_ANCHORS,
        "VM persistent actor timer foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/resource.rs",
        REQUIRED_RESOURCE_ANCHORS,
        "VM persistent actor resource foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_STORAGE_ANCHORS,
        "VM persistent actor storage foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        REQUIRED_STATE_ANCHORS,
        "VM persistent actor state snapshot foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs",
        REQUIRED_MIGRATION_ANCHORS,
        "VM persistent actor migration foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/persistent_actor_store.rs",
        REQUIRED_PERSISTENT_ACTOR_STORE_ANCHORS,
        "VM persistent actor store foundation",
    )?);
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor adversarial tests",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-persistent-actor-store", &diagnostics));
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
        "schema": "terlan-vm-persistent-actor-store-report-v1",
        "adapterMatrix": ADAPTER_MATRIX,
        "snapshotEventFixtures": SNAPSHOT_EVENT_FIXTURES,
        "replayTraces": REPLAY_TRACES,
        "schemaMigrationCases": SCHEMA_MIGRATION_CASES,
        "mailboxTimerRecovery": MAILBOX_TIMER_RECOVERY,
        "resourceHandleValidation": RESOURCE_HANDLE_VALIDATION,
        "crashInjectionOutcomes": CRASH_INJECTION_OUTCOMES,
        "rejectedPersistentActorPaths": REJECTED_PERSISTENT_ACTOR_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM persistent actor store report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorStoreSummary {
        adapter_matrix_count: ADAPTER_MATRIX.len(),
        snapshot_event_fixture_count: SNAPSHOT_EVENT_FIXTURES.len(),
        replay_trace_count: REPLAY_TRACES.len(),
        rejected_persistent_actor_path_count: REJECTED_PERSISTENT_ACTOR_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read persistent actor gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("adapter matrix", ADAPTER_MATRIX),
        ("snapshot/event fixtures", SNAPSHOT_EVENT_FIXTURES),
        ("replay traces", REPLAY_TRACES),
        ("schema migration cases", SCHEMA_MIGRATION_CASES),
        ("mailbox/timer recovery", MAILBOX_TIMER_RECOVERY),
        ("resource handle validation", RESOURCE_HANDLE_VALIDATION),
        ("crash injection outcomes", CRASH_INJECTION_OUTCOMES),
        (
            "rejected persistent actor paths",
            REJECTED_PERSISTENT_ACTOR_PATHS,
        ),
    ] {
        diagnostics.extend(validate_entries_for_placeholder_terms(label, entries));
    }
    diagnostics
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM persistent actor store {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_persistent_actor_store_test.rs"]
#[cfg(test)]
mod vm_persistent_actor_store_test;
