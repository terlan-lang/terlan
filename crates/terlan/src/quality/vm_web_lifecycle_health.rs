use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-lifecycle-health-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_SERVE_STARTUP_ANCHORS: &[&str] = &[
    "startup_tx",
    "startup_rx",
    "failed to receive server startup status",
    "server startup",
    "runtime_tls_config_for_serve",
];

const REQUIRED_COMPOSE_HEALTH_ANCHORS: &[&str] = &[
    "validate_postgres_healthcheck",
    "must define a healthcheck",
    "healthcheck must not be disabled",
    "healthcheck must define a non-empty test command",
    "healthcheck_has_enabled_test",
    "healthcheck_command_is_enabled",
];

const REQUIRED_COMPOSE_TEST_ANCHORS: &[&str] = &[
    "validate_project_compose_rejects_postgres_without_healthcheck",
    "validate_project_compose_rejects_disabled_postgres_healthcheck",
    "validate_project_compose_rejects_postgres_healthcheck_without_test",
    "validate_project_compose_rejects_postgres_healthcheck_none_test",
    "validate_project_compose_accepts_postgres_dev_service",
];

const REQUIRED_TLS_READINESS_ANCHORS: &[&str] = &[
    "Maximum order-state refresh attempts while waiting for ACME readiness",
    "challenge readiness",
    "Loads live TLS configuration for normal `terlc serve` startup",
    "acme_runtime_tls_config_for_serve",
    "issue_acme_certificate",
    "load_acme_runtime_tls_cache",
];

const REQUIRED_HTTP_LIFECYCLE_ANCHORS: &[&str] = &[
    "pub(crate) fn shutdown(",
    "pub(crate) fn shutdown_with_tls(",
    "tcp.close_listener",
    "self.handlers.drain(..)",
    "finish_http1_tcp_handler",
    "remove_listener_plan",
    "VmExitReason",
];

const REQUIRED_HTTP_LIFECYCLE_TEST_ANCHORS: &[&str] = &[
    "vm_http_tcp_server_cancels_parked_handler_and_closes_stream",
    "vm_http_tcp_server_shutdown_closes_listener_and_active_handlers",
    "vm_http_tcp_server_noop_poll_and_empty_shutdown_are_stable",
    "vm_http_tcp_server_inspects_listener_pressure_and_handler_counters",
    "vm_http_tcp_server_shutdown_with_tls_removes_listener_plan",
];

const REQUIRED_STREAM_LIFECYCLE_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/sse_test.rs",
        &[
            "vm_sse_stream_close_rejects_new_events_but_flushes_pending",
            "stream.close()",
            "stream.inspect().closed",
            "flush queued",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/websocket.rs",
        &[
            "VmWebSocketCloseOutcome",
            "VmWebSocketTerminationReason",
            "VmWebSocketTermination",
            "Timeout",
            "Cancelled",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/websocket_test.rs",
        &[
            "vm_websocket_session_closes_after_received_close_frame",
            "vm_websocket_session_closes_after_sent_close_frame",
            "vm_websocket_session_send_frame_closes_on_close_event",
            "vm_websocket_runtime_remove_inactive_stream_sessions_prunes_closed_and_cancelled_streams",
        ],
    ),
];

const REQUIRED_HOT_RELOAD_ANCHORS: &[&str] = &[
    "VmSourceReloadBatchReport",
    "changed_paths",
    "unique_source_paths",
    "ignored_paths",
    "duplicate_source_paths",
    "publish_changed_files_with_report",
    "atomic batch boundary",
    "event_snapshots",
];

const REQUIRED_HOT_RELOAD_TEST_ANCHORS: &[&str] = &[
    "source_reload_adapter_publishes_changed_terlan_file_generations",
    "source_reload_adapter_publishes_only_sources_from_mixed_batch",
    "source_reload_adapter_rejects_invalid_mixed_batch_without_partial_publication",
    "source_reload_adapter_reports_mixed_batch_diagnostics",
];

const REQUIRED_SUPPORT_EVIDENCE_ANCHORS: &[&str] = &[
    "vm_distributed_storage_force_local_writes_flushes_and_loads_snapshot",
    "vm_distributed_storage_reports_flush_timeout_with_retry_recovery",
    "requires_recovery",
    "retry_flush",
];

const REQUIRED_OPERATOR_ANCHORS: &[&str] = &[
    "HotReload",
    "NodeDrain",
    "ServiceRestart",
    "VmOperatorPolicy",
    "audit_required",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-lifecycle-health-check: vm-web-observability-check",
    "$(MAKE) web-compose-check",
    "$(MAKE) http-tls-check",
    "$(MAKE) vm-source-hot-reload-check",
    "vm_web_lifecycle_health_test",
    "vm-web-lifecycle-health",
];

const LIFECYCLE_STATE_TRANSITIONS: &[&str] = &[
    "startup: config validation -> package loading -> TLS plan -> listener bind",
    "ready: dependency healthcheck -> artifact loaded -> listener accepting",
    "draining: listener closed -> active handlers cancelled or drained",
    "shutdown: streams closed -> resources released -> telemetry flushed",
    "hot reload: changed source batch -> compile transaction -> code-server generation",
];

const HEALTH_ENDPOINT_FIXTURES: &[&str] = &[
    "HTTP health route matching exists through manifest handler tests",
    "Compose Postgres healthcheck required before dependency startup",
    "ACME/TLS readiness validates cached or live certificate material",
    "VM HTTP listener inspect exposes closed/queued/active handler state",
];

const DEPENDENCY_READINESS_DECISIONS: &[&str] = &[
    "Docker Compose is optional for standalone web packages",
    "Postgres service must exist when Compose is present",
    "Postgres healthcheck must be enabled and commandful",
    "TLS startup must validate cache or ACME issuance state",
    "VM artifact readiness remains tied to source-map/debug validation gates",
];

const DRAIN_TRACES: &[&str] = &[
    "HTTP shutdown closes listener before draining handlers",
    "HTTP shutdown drains retained handler states",
    "HTTP shutdown exits handler processes with typed VmExitReason",
    "TLS shutdown removes listener-bound plan",
    "WebSocket inactive-stream pruning removes closed/cancelled sessions",
];

const SHUTDOWN_TRACES: &[&str] = &[
    "SSE close rejects new events but flushes pending events",
    "WebSocket receive close marks session closed",
    "WebSocket send close marks session closed",
    "VM TCP closed streams reject late sends",
    "distributed storage flush timeout carries retry recovery evidence",
];

const HOT_RELOAD_TRACES: &[&str] = &[
    "source reload publishes initial generation",
    "source reload hot-reloads changed Terlan file generations",
    "source reload ignores non-Terlan paths",
    "source reload reports mixed batch path counts",
    "source reload rejects invalid batches without partial publish",
];

const FORCE_KILL_EVIDENCE: &[&str] = &[
    "VmExitReason::Killed can be carried through HTTP shutdown",
    "WebSocket termination distinguishes Timeout and Cancelled",
    "NodeDrain operator action is declared for future guarded operator mode",
    "force-kill support-bundle capture remains rejected until implemented",
];

const REJECTED_LIFECYCLE_PATHS: &[&str] = &[
    "public /ready and /live endpoints generated for every VM web app",
    "startup state exported consistently through CLI, HTTP, metrics, traces, and editor surfaces",
    "drain timeout policy for in-flight request and streaming response",
    "support-bundle capture for forced kill",
    "telemetry flush failure recovery",
    "hot reload during live-template stream with typed stream continuity policy",
    "native work cancellation while draining",
    "dependency-health wait loop instead of static Compose validation only",
    "stateful actor warmup readiness",
    "production liveness/readiness schema stability",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmWebLifecycleHealthSummary {
    pub lifecycle_state_transition_count: usize,
    pub health_endpoint_fixture_count: usize,
    pub drain_trace_count: usize,
    pub rejected_lifecycle_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_web_lifecycle_health(root: &Path) -> QualityResult<VmWebLifecycleHealthSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/mod.rs",
        REQUIRED_SERVE_STARTUP_ANCHORS,
        "serve startup lifecycle",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/compose_check.rs",
        REQUIRED_COMPOSE_HEALTH_ANCHORS,
        "dependency health validation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/compose_test.rs",
        REQUIRED_COMPOSE_TEST_ANCHORS,
        "dependency health tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/tls.rs",
        REQUIRED_TLS_READINESS_ANCHORS,
        "TLS readiness lifecycle",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http.rs",
        REQUIRED_HTTP_LIFECYCLE_ANCHORS,
        "VM HTTP shutdown lifecycle",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_test.rs",
        REQUIRED_HTTP_LIFECYCLE_TEST_ANCHORS,
        "VM HTTP lifecycle tests",
    )?);
    for (relative, anchors) in REQUIRED_STREAM_LIFECYCLE_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM web stream lifecycle",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/source_reload.rs",
        REQUIRED_HOT_RELOAD_ANCHORS,
        "VM source hot-reload lifecycle",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/source_reload_test.rs",
        REQUIRED_HOT_RELOAD_TEST_ANCHORS,
        "VM source hot-reload tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        REQUIRED_SUPPORT_EVIDENCE_ANCHORS,
        "flush and recovery evidence",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/vm/instrumentation.rs",
        REQUIRED_OPERATOR_ANCHORS,
        "operator lifecycle controls",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "lifecycle state transitions",
        LIFECYCLE_STATE_TRANSITIONS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "health endpoint fixtures",
        HEALTH_ENDPOINT_FIXTURES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "dependency readiness decisions",
        DEPENDENCY_READINESS_DECISIONS,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "drain traces",
        DRAIN_TRACES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "shutdown traces",
        SHUTDOWN_TRACES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "hot-reload traces",
        HOT_RELOAD_TRACES,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "force-kill evidence",
        FORCE_KILL_EVIDENCE,
    ));
    diagnostics.extend(validate_no_placeholder_report_entries(
        "rejected lifecycle paths",
        REJECTED_LIFECYCLE_PATHS,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-web-lifecycle-health", &diagnostics));
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
        "schema": "terlan-vm-web-lifecycle-health-report-v1",
        "lifecycleStateTransitions": LIFECYCLE_STATE_TRANSITIONS,
        "healthEndpointFixtures": HEALTH_ENDPOINT_FIXTURES,
        "dependencyReadinessDecisions": DEPENDENCY_READINESS_DECISIONS,
        "drainTraces": DRAIN_TRACES,
        "shutdownTraces": SHUTDOWN_TRACES,
        "hotReloadTraces": HOT_RELOAD_TRACES,
        "forceKillEvidence": FORCE_KILL_EVIDENCE,
        "rejectedLifecyclePaths": REJECTED_LIFECYCLE_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM web lifecycle health report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebLifecycleHealthSummary {
        lifecycle_state_transition_count: LIFECYCLE_STATE_TRANSITIONS.len(),
        health_endpoint_fixture_count: HEALTH_ENDPOINT_FIXTURES.len(),
        drain_trace_count: DRAIN_TRACES.len(),
        rejected_lifecycle_path_count: REJECTED_LIFECYCLE_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read VM web lifecycle health gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM web lifecycle health gate term `{term}`"))
        .collect())
}

pub fn validate_no_placeholder_report_entries(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM web lifecycle health {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_web_lifecycle_health_test.rs"]
mod vm_web_lifecycle_health_test;
