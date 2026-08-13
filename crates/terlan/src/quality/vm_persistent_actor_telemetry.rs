use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-telemetry-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &[
            "VmDistributedStorageOutcome",
            "SnapshotLoaded",
            "ChecksumMismatch",
            "PartialWrite",
            "FlushTimedOut",
            "StaleSnapshot",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/coordination.rs",
        &[
            "trace_id",
            "VmCoordinationEnvelope",
            "VmDistributedTransportFrame",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/code_server.rs",
        &[
            "event_snapshots",
            "VmCodeServerEventSnapshot",
            "source_map_id",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/failure.rs",
        &[
            "VmFailureReport",
            "delivered_exit_signals",
            "delivered_down_messages",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/process.rs",
        &[
            "mailbox: VecDeque<VmMessage>",
            "resource_handles",
            "mailbox_len",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer.rs",
        &[
            "VmTimerSnapshot",
            "owner: VmProcessId",
            "deadline_tick: u64",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource.rs",
        &[
            "VmResourceSnapshot",
            "owner: VmProcessId",
            "transfer_policy",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_telemetry.rs",
        &[
            "VmPersistentActorTelemetrySpan",
            "validate_persistent_actor_telemetry_trace",
            "MisleadingSuccessAfterFailure",
            "UnredactedSecret",
            "deterministic_restore_trace",
            "VmPersistentActorTelemetryCollector",
            "VmPersistentActorTelemetryLifecycle",
            "VmPersistentActorDebuggerHandoff",
            "persistent_actor_debugger_handoff",
            "VmPersistentActorTelemetrySupportPolicy",
            "VmPersistentActorTelemetrySupportBundle",
            "persistent_actor_telemetry_support_bundle",
            "publish_model_sync_changes",
            "ModelSyncSequenceRegression",
            "EmptyModelSyncStream",
            "VmPersistentActorTelemetryLimits",
            "TelemetryAfterRollback",
            "CardinalityLimitExceeded",
            "ActorIdentityMismatch",
            "CounterOverflow",
            "MissingSourceMapIdentity",
            "ReplayStepUnavailable",
            "MailboxRestore",
            "TimerRestore",
            "PostRecoveryMessage",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_telemetry_aggregation.rs",
        &[
            "VmPersistentActorMetricAggregator",
            "VmPersistentActorMetricLimits",
            "VmPersistentActorMetricSeries",
            "CardinalityLimitExceeded",
            "CounterOverflow",
            "ingest_trace",
        ],
    ),
    (
        "std/vm/Fault.terl",
        &[
            "classification",
            "rollback descriptor",
            "pub failure(rollback: Rollback)",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_reports_finalize_and_partial_write_failures",
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_rejects_stale_snapshot_replay",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/coordination_test.rs",
        &["trace:vm-a:vm-b:1"],
    ),
    (
        "crates/terlan/src/runtime/vm/code_server_test.rs",
        &["source_hot_reload_records_reload_and_rollback_events_for_inspection"],
    ),
    (
        "crates/terlan/src/runtime/vm/failure_test.rs",
        &["failure_runtime_reports_missing_exited_and_self_link_diagnostics"],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_telemetry_test.rs",
        &[
            "vm_persistent_actor_telemetry_accepts_deterministic_restore_trace",
            "vm_persistent_actor_telemetry_preserves_typed_failure_classification",
            "vm_persistent_actor_telemetry_rejects_duplicate_and_out_of_order_spans",
            "vm_persistent_actor_telemetry_rejects_missing_identity_and_bad_ranges",
            "vm_persistent_actor_telemetry_rejects_secret_leak_and_success_after_failure",
            "vm_persistent_actor_telemetry_collector_emits_operation_spans_with_redaction",
            "vm_persistent_actor_telemetry_collector_propagates_failure_and_stops_after_rollback",
            "vm_persistent_actor_telemetry_collector_enforces_cardinality_limits",
            "vm_persistent_actor_telemetry_rejects_mixed_identity_and_counter_overflow",
            "vm_persistent_actor_telemetry_lifecycle_emits_store_and_restore_spans",
            "vm_persistent_actor_telemetry_lifecycle_rejects_identity_drift_and_traces_failures",
            "vm_persistent_actor_telemetry_builds_typed_debugger_handoff",
            "vm_persistent_actor_telemetry_rejects_invalid_debugger_handoff",
            "vm_persistent_actor_telemetry_exports_structurally_redacted_support_bundle",
            "vm_persistent_actor_telemetry_support_bundle_rejects_invalid_trace",
            "vm_persistent_actor_telemetry_publishes_ordered_model_sync_stream",
            "vm_persistent_actor_telemetry_rejects_invalid_model_sync_stream_atomically",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_telemetry_aggregation_test.rs",
        &[
            "vm_persistent_actor_metrics_aggregate_cross_actor_without_actor_id_labels",
            "vm_persistent_actor_metrics_reject_limits_and_overflow_atomically",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-telemetry-check: vm-persistent-actor-performance-budget-check",
    "vm_persistent_actor_telemetry_accepts_deterministic_restore_trace",
    "vm_persistent_actor_telemetry_preserves_typed_failure_classification",
    "vm_persistent_actor_telemetry_rejects_duplicate_and_out_of_order_spans",
    "vm_persistent_actor_telemetry_rejects_missing_identity_and_bad_ranges",
    "vm_persistent_actor_telemetry_rejects_secret_leak_and_success_after_failure",
    "vm_persistent_actor_telemetry_collector_emits_operation_spans_with_redaction",
    "vm_persistent_actor_telemetry_collector_propagates_failure_and_stops_after_rollback",
    "vm_persistent_actor_telemetry_collector_enforces_cardinality_limits",
    "vm_persistent_actor_telemetry_rejects_mixed_identity_and_counter_overflow",
    "vm_persistent_actor_telemetry_lifecycle_emits_store_and_restore_spans",
    "vm_persistent_actor_telemetry_lifecycle_rejects_identity_drift_and_traces_failures",
    "vm_persistent_actor_telemetry_builds_typed_debugger_handoff",
    "vm_persistent_actor_telemetry_rejects_invalid_debugger_handoff",
    "vm_persistent_actor_telemetry_exports_structurally_redacted_support_bundle",
    "vm_persistent_actor_telemetry_support_bundle_rejects_invalid_trace",
    "vm_persistent_actor_telemetry_publishes_ordered_model_sync_stream",
    "vm_persistent_actor_telemetry_rejects_invalid_model_sync_stream_atomically",
    "vm_persistent_actor_metrics_aggregate_cross_actor_without_actor_id_labels",
    "vm_persistent_actor_metrics_reject_limits_and_overflow_atomically",
    "vm_persistent_actor_telemetry_test",
    "vm-persistent-actor-telemetry",
];

const TRACE_FIXTURES: &[&str] = &[
    "append span includes actor id, adapter id, sequence, checksum",
    "snapshot span includes actor family, snapshot generation, durable bytes",
    "checkpoint span includes event range and scheduler ticks",
    "replay span includes source snapshot and replayed event range",
    "restore span includes recovery phase and typed failure reason",
    "resource validation span includes redacted resource labels",
    "model-sync publication span includes stream cursor",
];

const SPAN_SCHEMAS: &[&str] = &[
    "actor_id",
    "actor_family",
    "schema_id",
    "snapshot_generation",
    "event_range",
    "adapter_id",
    "scheduler_ticks",
    "durable_bytes",
    "retry_count",
    "recovery_phase",
    "typed_failure_reason",
];

const REDACTION_CASES: &[&str] = &[
    "resource handle labels are redacted before telemetry emission",
    "adapter internals are not emitted",
    "mailbox payloads are classified before support bundle export",
    "secret-bearing model-sync updates are rejected until policy exists",
    "failure reasons preserve type while removing raw host paths",
];

const REPLAY_TIMELINES: &[&str] = &[
    "load snapshot",
    "replay events",
    "restore mailbox",
    "restore timers",
    "validate resource handles",
    "deliver first post-recovery message",
];

const DEBUGGER_HANDOFF_METADATA: &[&str] = &[
    "source_map_id",
    "replay_step",
    "actor_id",
    "snapshot_generation",
    "typed_failure_reason",
];

const FAILURE_CLASSIFICATIONS: &[&str] = &[
    "checksum_mismatch",
    "partial_write",
    "flush_timeout",
    "stale_snapshot",
    "storage_unavailable",
    "rollback_after_partial_commit",
];

const CARDINALITY_CHECKS: &[&str] = &[
    "actor id is bounded by actor count",
    "actor family is bounded by package schema",
    "adapter id is bounded by configured adapters",
    "failure reason is bounded by typed VM variants",
    "schema id is bounded by migration graph",
];

const DETERMINISTIC_TRACE_VALIDATION_CASES: &[&str] = &[
    "deterministic restore replay timeline",
    "typed failure classification preservation",
    "duplicate and out-of-order span rejection",
    "missing identity and invalid event range rejection",
    "secret leak and success-after-failure rejection",
    "typed operation span emission with deterministic sequencing and redaction",
    "typed failure propagation and post-rollback suppression",
    "schema and adapter cardinality limit enforcement",
    "mixed actor identity and aggregate counter overflow rejection",
    "automatic store append snapshot checkpoint and restore lifecycle telemetry",
    "lifecycle actor identity drift and partial write failure rejection",
    "typed debugger handoff from a validated replay step",
    "missing source map unavailable replay step and malformed trace rejection",
    "structurally redacted support bundle export",
    "support bundle invalid trace and cross actor rejection",
    "ordered model sync stream publication without row payload leakage",
    "atomic empty invalid and regressed model sync stream rejection",
    "bounded cross actor aggregation without actor id metric labels",
    "atomic family series and counter cardinality rejection",
];

const REJECTED_TELEMETRY_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm persistent actor telemetry summary.
pub struct VmPersistentActorTelemetrySummary {
    pub trace_fixture_count: usize,
    pub span_schema_field_count: usize,
    pub deterministic_trace_validation_count: usize,
    pub replay_timeline_count: usize,
    pub rejected_telemetry_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm persistent actor telemetry.
pub fn run_vm_persistent_actor_telemetry(
    root: &Path,
) -> QualityResult<VmPersistentActorTelemetrySummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor telemetry foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor telemetry fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-persistent-actor-telemetry",
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
        "schema": "terlan-vm-persistent-actor-telemetry-report-v1",
        "traceFixtures": TRACE_FIXTURES,
        "spanSchemas": SPAN_SCHEMAS,
        "redactionCases": REDACTION_CASES,
        "replayTimelines": REPLAY_TIMELINES,
        "debuggerHandoffMetadata": DEBUGGER_HANDOFF_METADATA,
        "failureClassifications": FAILURE_CLASSIFICATIONS,
        "metricCardinalityChecks": CARDINALITY_CHECKS,
        "deterministicTraceValidationCases": DETERMINISTIC_TRACE_VALIDATION_CASES,
        "rejectedTelemetryPaths": REJECTED_TELEMETRY_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize VM persistent actor telemetry report: {err}")
    })?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorTelemetrySummary {
        trace_fixture_count: TRACE_FIXTURES.len(),
        span_schema_field_count: SPAN_SCHEMAS.len(),
        deterministic_trace_validation_count: DETERMINISTIC_TRACE_VALIDATION_CASES.len(),
        replay_timeline_count: REPLAY_TIMELINES.len(),
        rejected_telemetry_path_count: REJECTED_TELEMETRY_PATHS.len(),
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
        format!("Makefile: failed to read persistent actor telemetry gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor telemetry gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("trace fixtures", TRACE_FIXTURES),
        ("span schemas", SPAN_SCHEMAS),
        ("redaction cases", REDACTION_CASES),
        ("replay timelines", REPLAY_TIMELINES),
        ("debugger handoff metadata", DEBUGGER_HANDOFF_METADATA),
        ("failure classifications", FAILURE_CLASSIFICATIONS),
        ("metric cardinality checks", CARDINALITY_CHECKS),
        (
            "deterministic trace validation cases",
            DETERMINISTIC_TRACE_VALIDATION_CASES,
        ),
        ("rejected telemetry paths", REJECTED_TELEMETRY_PATHS),
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
                        "VM persistent actor telemetry {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_persistent_actor_telemetry_test.rs"]
#[cfg(test)]
mod vm_persistent_actor_telemetry_test;
