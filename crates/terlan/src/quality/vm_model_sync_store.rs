use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-model-sync-store-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_ACTOR_ANCHORS: &[&str] = &[
    "pub(crate) struct VmActorRuntime",
    "spawn_root",
    "spawn_child",
    "register_name",
    "send_named",
    "receive_next_or_block",
    "selective_receive_or_block",
    "receive_with_timeout",
];

const REQUIRED_DISTRIBUTED_STATE_ANCHORS: &[&str] = &[
    "pub(crate) enum VmDistributedStatePolicy",
    "pub(crate) struct VmDistributedStateScope",
    "pub(crate) struct VmDistributedStateVersion",
    "pub(crate) struct VmDistributedStateEntry",
    "pub(crate) struct VmDistributedStateConflict",
    "pub(crate) enum VmDistributedStateWriteOutcome",
    "write(",
    "export_snapshot",
    "import_snapshot",
    "Replayed",
    "Conflict",
    "PolicyMismatch",
];

const REQUIRED_DISTRIBUTED_STORAGE_ANCHORS: &[&str] = &[
    "pub(crate) enum VmDistributedStorageMode",
    "pub(crate) enum VmDistributedStorageOperation",
    "pub(crate) struct VmDistributedStoragePolicy",
    "pub(crate) struct VmDistributedStorageSnapshot",
    "pub(crate) enum VmDistributedStorageOutcome",
    "pub(crate) struct VmDistributedStorageAdapter",
    "PartialWrite",
    "FlushTimedOut",
    "StaleSnapshot",
    "requires_recovery",
    "recovery_action",
    "replicate_snapshot",
];

const REQUIRED_MODEL_SYNC_ANCHORS: &[&str] = &[
    "pub(crate) struct VmModelSyncKey",
    "pub(crate) struct VmModelSyncVersion",
    "pub(crate) enum VmModelSyncChangeKind",
    "pub(crate) struct VmModelSyncChange",
    "pub(crate) struct VmModelSyncRow",
    "pub(crate) enum VmModelSyncOutcome",
    "pub(crate) enum VmModelSyncProjectedFieldType",
    "pub(crate) struct VmModelSyncRowFieldProjection",
    "pub(crate) struct VmModelSyncRowProjection",
    "pub(crate) struct VmModelSyncTemplateSubscription",
    "pub(crate) struct VmModelSyncTemplateInvalidation",
    "pub(crate) enum VmModelSyncAdapterCapability",
    "pub(crate) struct VmModelSyncAdapterContract",
    "pub(crate) struct VmSyncableModelDeclaration",
    "pub(crate) enum VmModelSyncPermissionOperation",
    "pub(crate) struct VmModelSyncFieldPermission",
    "pub(crate) struct VmModelSyncPermissionPolicy",
    "pub(crate) trait VmModelSyncStoreAdapter",
    "pub(crate) struct VmInMemoryModelSyncStore",
    "invalidate_live_template_subscribers_from_model_events",
    "validate_non_postgres_model_sync_adapter_contracts",
    "validate_model_sync_permission_drift",
    "project_model_sync_row_from_adapter_fields",
    "syncable model name must be non-empty",
    "expected_version",
    "changes_since",
    "export_snapshot",
    "Conflict",
    "Deleted",
];

const REQUIRED_POSTGRES_ANCHORS: &[&str] = &[
    "pub struct Pool",
    "pub fn connect",
    "pub fn query",
    "pub fn query_one",
    "pub fn execute",
    "pub fn batch_execute",
    "pub fn transaction",
    "deadpool_postgres",
    "build_deadpool",
];

const REQUIRED_POSTGRES_ROW_ANCHORS: &[&str] = &[
    "pub struct Row",
    "enum PostgresValue",
    "pub fn string",
    "pub fn int",
    "pub fn json",
    "pub(super) fn row_from_driver",
];

const REQUIRED_POSTGRES_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/native/postgres_test.rs",
        &[
            "config_builders_set_pool_limits_and_timeouts",
            "validate_config_rejects_invalid_pool_settings",
            "query_operations_reject_empty_sql_before_adapter_dispatch",
            "transaction_returns_stable_driver_connection_error",
            "row_accessors_decode_matching_values",
            "row_accessors_report_missing_and_type_errors",
        ],
    ),
    (
        "crates/terlan/src/runtime/native_boundary/dispatch_test.rs",
        &[
            "dispatch_postgres_query_operations_are_known_driver_operations",
            "dispatch_postgres_transaction_requires_runtime_bridge",
            "dispatch_postgres_row_accessors_decode_values",
            "bridge_dispatch_postgres_row_handles_decode_values",
        ],
    ),
    (
        "crates/terlan/src/commands/db/execution_test.rs",
        &["failed migration SQL cannot be followed by a committed history"],
    ),
];

const REQUIRED_VM_STATE_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/distributed_state_test.rs",
        &[
            "vm_distributed_state_reports_conflicts_with_versions_and_policy",
            "vm_distributed_state_exports_and_imports_deterministic_snapshots",
            "vm_distributed_state_rejects_invalid_scopes_versions_and_snapshots",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_rejects_stale_snapshot_replay",
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_reports_finalize_and_partial_write_failures",
            "vm_distributed_storage_reports_flush_timeout_with_retry_recovery",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/model_sync_test.rs",
        &[
            "vm_model_sync_store_applies_updates_and_exports_deterministic_snapshot",
            "vm_model_sync_store_rejects_stale_versions_without_mutation",
            "vm_model_sync_store_emits_delete_tombstones_and_change_streams",
            "vm_model_sync_store_rejects_invalid_keys_and_versions",
            "vm_model_sync_declares_syncable_model_without_orm_identity_map",
            "vm_model_sync_rejects_invalid_syncable_model_declarations",
            "vm_model_sync_invalidates_live_template_subscribers_from_committed_events",
            "vm_model_sync_rejects_invalid_live_template_subscription_identity",
            "vm_model_sync_projects_adapter_row_into_typed_model_row",
            "vm_model_sync_row_projection_rejects_missing_adapter_field",
            "vm_model_sync_row_projection_rejects_type_mismatch",
            "vm_model_sync_row_projection_rejects_invalid_version_sequence",
            "vm_model_sync_row_projection_rejects_duplicate_model_fields",
            "vm_model_sync_permission_policy_accepts_allowed_model_and_field_changes",
            "vm_model_sync_permission_policy_rejects_missing_model_policy",
            "vm_model_sync_permission_policy_rejects_field_level_drift",
            "vm_model_sync_permission_policy_rejects_denied_delete_operation",
            "vm_model_sync_validates_non_postgres_adapter_portability_contracts",
            "vm_model_sync_rejects_non_postgres_adapter_missing_portable_capability",
            "vm_model_sync_rejects_non_postgres_adapter_leaking_postgres_capability",
        ],
    ),
    (
        "std/vm/DistributedStateTest.terl",
        &[
            "write_outcomes_are_explicit_and_versioned",
            "conflict_and_policy_mismatch_are_typed",
            "restore(snapshot)",
        ],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "force_local_adapter_writes_flushes_and_loads_checkpoint",
            "reopen_preserves_snapshots_and_sequence_watermark",
            "assert_equal(\"stale_snapshot\", stale_kind)",
        ],
    ),
    (
        "std/vm/ModelSyncTest.terl",
        &[
            "optimistic_write_plan_is_source_visible",
            "persistent_actor_adapter_is_source_visible",
            "package_store_adapter_is_source_visible",
            "expected_version",
            "next_version",
            "ModelSync.conflict",
            "ModelSync.adapter_contract",
            "ModelSync.persistent_actor_adapter",
            "ModelSync.package_store_adapter",
            "SyncableModel",
            "syncable_model_declaration_is_source_visible",
        ],
    ),
];

const REQUIRED_LIVE_TEMPLATE_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/http/LiveChannelTest.terl",
        &[
            "Router.sse",
            "Sse.endpoint_with_keep_alive",
            "live_channel_sse_handler_records_queued_events",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/sse.rs",
        &["VmSseEndpointPlan", "VmSseStream", "flush_next"],
    ),
    (
        "crates/terlan/src/commands/serve/watch.rs",
        &["ReloadHub", "broadcast_reload", "subscribers.retain"],
    ),
];

const REQUIRED_PROPERTY_TEST_ANCHORS: &[&str] = &[
    "StatefulPropertyTest",
    "property checks can exercise",
    "for_all",
    "property",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-model-sync-store-check: vm-web-route-schema-client-check",
    "$(MAKE) native-boundary-postgres-check",
    "$(MAKE) db-command-check",
    "$(TERLC) test std/vm/ModelSyncTest.terl",
    "formal_pipeline::formal_pipeline_test::persistence_and_effect_interfaces::embedded_std_interfaces_include_vm_model_sync_contract",
    "runtime::vm::model_sync::model_sync_test::vm_model_sync_store_rejects_stale_versions_without_mutation",
    "runtime::vm::model_sync::model_sync_test::vm_model_sync_projects_adapter_row_into_typed_model_row",
    "runtime::vm::model_sync::model_sync_test::vm_model_sync_permission_policy_rejects_field_level_drift",
    "vm_model_sync_store_test",
    "vm-model-sync-store",
];

const MODEL_FIXTURES: &[&str] = &[
    "actor-backed state fixture: named VM actor mailbox and receive semantics",
    "distributed state fixture: typed scope, owner, version, policy, snapshot",
    "distributed storage fixture: append, flush, load, compact, recovery outcome",
    "model sync fixture: typed key, optimistic version, change event, snapshot",
    "Postgres row fixture: maintained adapter plus typed row decoding",
    "live-template subscriber fixture: SSE channel and reload fan-out",
    "syncable model declaration fixture: typed public model descriptor without ORM identity map",
];

const ADAPTER_MATRIX: &[&str] = &[
    "actor-store: VM actor process and mailbox ownership",
    "model-sync-store: VM-owned in-memory typed adapter contract",
    "distributed-state-store: VM-owned in-memory versioned state table",
    "distributed-storage-store: VM-owned snapshot adapter and recovery outcomes",
    "postgres-store: generated libpq C ABI adapter, VM-owned pool, and typed rows",
    "non-Postgres portability: typed adapter contracts reject missing portable capabilities",
    "persistent-actor-store: source-visible persistent actor adapter binding",
    "package-store: source-visible package store adapter binding",
    "syncable-model: source-visible typed model declaration without ORM behavior",
];

const VERSION_CONFLICT_CASES: &[&str] = &[
    "fresh write applies with typed version metadata",
    "Terlan-facing optimistic concurrency API builds expected and next versions",
    "identical write replays without publishing a new state",
    "stale write returns typed Conflict with local and incoming versions",
    "policy mismatch is typed and does not mutate state",
    "model sync delete emits typed tombstone without retaining hidden state",
    "snapshot replay is rejected as stale storage input",
];

const CHANGE_STREAM_TRACES: &[&str] = &[
    "model sync change stream carries created, updated, and deleted events",
    "SSE endpoint carries queue and keep-alive policy",
    "live reload hub broadcasts to retained subscribers",
    "distributed storage append records checkpoint sequence and checksum",
    "distributed state snapshot exports deterministic scope order",
];

const PERMISSION_CHECKS: &[&str] = &[
    "route/schema gate is sequenced before model sync store gate",
    "database commands validate dev-only rebuild before destructive execution",
    "model-sync permission policies reject model and field-level drift",
    "subscriber permissions remain rejected until live-template policy rows exist",
];

const TRANSACTION_CASES: &[&str] = &[
    "Postgres connect/query/query-one/execute use maintained adapter",
    "Postgres transaction starts, commits, and reports driver rollback errors",
    "migration execution rejects committed history after failed migration SQL",
    "row decoding reports missing columns and type errors",
    "row-to-model generation projects typed adapter rows into sync rows",
];

const LIVE_TEMPLATE_PROPAGATION: &[&str] = &[
    "live channel response records queued SSE events",
    "SSE endpoint can be routed through std.http.Router",
    "reload hub removes failed subscribers during broadcast",
    "committed model events invalidate typed live-template subscribers",
];

const ROLLBACK_BEHAVIOR: &[&str] = &[
    "partial write does not advance storage watermark",
    "flush timeout requires retry and preserves previous sequence",
    "checksum mismatch requires snapshot repair",
    "transaction rollback is reported through the Postgres adapter",
];

const REJECTED_MODEL_SYNC_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm model sync store summary.
pub struct VmModelSyncStoreSummary {
    pub model_fixture_count: usize,
    pub adapter_matrix_count: usize,
    pub version_conflict_case_count: usize,
    pub rejected_model_sync_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm model sync store.
pub fn run_vm_model_sync_store(root: &Path) -> QualityResult<VmModelSyncStoreSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/actor.rs",
        REQUIRED_ACTOR_ANCHORS,
        "VM actor model-store foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        REQUIRED_DISTRIBUTED_STATE_ANCHORS,
        "VM distributed state model-store foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_DISTRIBUTED_STORAGE_ANCHORS,
        "VM distributed storage model-store foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/model_sync.rs",
        REQUIRED_MODEL_SYNC_ANCHORS,
        "VM model sync adapter foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/native/postgres.rs",
        REQUIRED_POSTGRES_ANCHORS,
        "Postgres model-store adapter foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/native/postgres/row.rs",
        REQUIRED_POSTGRES_ROW_ANCHORS,
        "Postgres typed row decoding foundation",
    )?);
    for (relative, anchors) in REQUIRED_POSTGRES_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "Postgres model-store tests",
        )?);
    }
    for (relative, anchors) in REQUIRED_VM_STATE_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM versioned state and storage tests",
        )?);
    }
    for (relative, anchors) in REQUIRED_LIVE_TEMPLATE_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "live template propagation foundation",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "std/test/StatefulPropertyTest.terl",
        REQUIRED_PROPERTY_TEST_ANCHORS,
        "stateful property test foundation",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-model-sync-store", &diagnostics));
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
        "schema": "terlan-vm-model-sync-store-report-v1",
        "modelFixtures": MODEL_FIXTURES,
        "adapterMatrix": ADAPTER_MATRIX,
        "versionConflictCases": VERSION_CONFLICT_CASES,
        "changeStreamTraces": CHANGE_STREAM_TRACES,
        "permissionChecks": PERMISSION_CHECKS,
        "transactionCases": TRANSACTION_CASES,
        "liveTemplatePropagation": LIVE_TEMPLATE_PROPAGATION,
        "rollbackBehavior": ROLLBACK_BEHAVIOR,
        "rejectedModelSyncPaths": REJECTED_MODEL_SYNC_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM model sync store report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmModelSyncStoreSummary {
        model_fixture_count: MODEL_FIXTURES.len(),
        adapter_matrix_count: ADAPTER_MATRIX.len(),
        version_conflict_case_count: VERSION_CONFLICT_CASES.len(),
        rejected_model_sync_path_count: REJECTED_MODEL_SYNC_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read model sync store gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing model sync store gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("model fixtures", MODEL_FIXTURES),
        ("adapter matrix", ADAPTER_MATRIX),
        ("version conflict cases", VERSION_CONFLICT_CASES),
        ("change stream traces", CHANGE_STREAM_TRACES),
        ("permission checks", PERMISSION_CHECKS),
        ("transaction cases", TRANSACTION_CASES),
        ("live template propagation", LIVE_TEMPLATE_PROPAGATION),
        ("rollback behavior", ROLLBACK_BEHAVIOR),
        ("rejected model sync paths", REJECTED_MODEL_SYNC_PATHS),
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
                        "VM model sync store {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_model_sync_store_test.rs"]
#[cfg(test)]
mod vm_model_sync_store_test;
