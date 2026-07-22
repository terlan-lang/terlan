use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-web-observability-report.json";

const REQUIRED_SERVE_LOGGING_ANCHORS: &[&str] = &[
    "next_request_id",
    "connection_id_for_request",
    "request_id={request_id}",
    "connection_id={connection_id}",
    "build_id={build_id}",
    "method={request_method}",
    "path={request_path}",
    "route_method={}",
    "handler={}.{}",
    "status={status}",
    "duration_ms={duration_ms}",
    "source={}:{}:{}",
    "render_dev_error_page",
];

const REQUIRED_SERVE_TEST_ANCHORS: &[&str] = &[
    "render_handler_log_line_includes_handler_metadata",
    "render_handler_log_line_includes_optional_source_metadata",
    "render_static_log_line_includes_asset_metadata",
    "render_static_route_log_line_includes_route_metadata",
    "render_file_route_log_line_includes_route_and_file_metadata",
    "render_dev_error_page_includes_escaped_handler_metadata",
];

const REQUIRED_VM_HTTP_ANCHORS: &[&str] = &[
    "VmHttpQueueMetrics",
    "VmHttpTcpServerInfo",
    "VmHttpTcpServerPoll",
    "accepted_total",
    "completed_total",
    "skipped_blocked",
    "enqueue_wait_count",
    "enqueue_wait_total_ns",
    "inspect(&self, tcp: &VmTcpRuntime)",
    "handler_poll_limit",
];

const REQUIRED_VM_HTTP_TEST_ANCHORS: &[&str] = &[
    "vm_http_queue_preserves_fifo_order_and_metrics",
    "server.inspect(&tcp)",
    "accepted_total",
    "completed_total",
    "VM HTTP server handler poll limit must be greater than 0",
];

const REQUIRED_STREAM_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/sse.rs",
        &[
            "VmSseStreamInfo",
            "pending_events",
            "max_pending_events",
            "emitted_events",
            "inspect(&self) -> VmSseStreamInfo",
            "BackpressureExceeded",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/websocket.rs",
        &[
            "VmWebSocketInboundQueueInfo",
            "pending_frames",
            "queued_frame_bytes",
            "inspect(&self) -> VmWebSocketInboundQueueInfo",
            "error[vm_websocket_queue]: pending frame queue is full",
        ],
    ),
];

const REQUIRED_SOURCE_MAP_ANCHORS: &[&str] = &[
    "source_map_id",
    "Compiled module name and checksum/source-map metadata",
    "source_name",
];

const REQUIRED_VM_INSTRUMENTATION_ANCHORS: &[&str] = &[
    "VmRuntimeInspectionSnapshot",
    "VmProcessInspectionSnapshot",
    "VmDashboardRenderSnapshot",
    "process_registry",
    "mailboxes",
    "reductions",
    "resource_handles",
    "native_call_state",
];

const REQUIRED_BENCHMARK_ANCHORS: &[&str] = &[
    "terlan-vm-http-runtime",
    "http_runtime",
    "RuntimeCapabilityStatus::Available",
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-web-observability-check: vm-web-config-secret-boundary-check",
    "$(MAKE) http-observability-check",
    "$(MAKE) vm-diagnostics-quality-check",
    "vm_web_observability_test",
    "vm-web-observability",
];

const TELEMETRY_SCHEMA: &[&str] = &[
    "request_id",
    "connection_id",
    "actor_id",
    "route_id",
    "template_stream_id.rejectedUntilLiveTemplateRuntimeEmission",
    "security_policy_decision",
    "config_profile",
    "source_map_location",
    "duration_ms",
    "status",
];

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const ROUTE_TRACES: &[&str] = &[
    "dynamic handler log line with request id, connection id, route, handler, status, duration, source span",
    "static asset log line with request id, connection id, asset path, status, duration",
    "static manifest route log line with source span",
    "file route log line with route, file, status, duration, source span",
    "development error page with escaped source-aware metadata",
];

const STREAM_TRACES: &[&str] = &[
    "SSE stream inspect exposes pending/emitted event counts",
    "SSE stream reports bounded backpressure",
    "WebSocket inbound queue inspect exposes pending frame and byte pressure",
    "WebSocket inbound queue reports bounded backpressure",
    "template stream id remains rejected until live-template runtime emits it",
];

const SECURITY_DECISION_TRACES: &[&str] = &[
    "Slice 115 policy report is prerequisite",
    "cookie/header/TLS policy decisions are enforced before observability",
    "security decision telemetry remains rejected until HTTP runtime carries typed policy ids",
];

const REDACTION_CHECKS: &[&str] = &[
    "serve dev error page escapes request, route, handler, source, and backend text",
    "config secret boundary report is prerequisite",
    "path redaction remains rejected until support-bundle redaction exists",
    "user data redaction remains rejected until typed telemetry fields classify payload data",
];

const CORRELATION_CHECKS: &[&str] = &[
    "serve logging correlates request_id with build_id, route, handler, and source",
    "VM HTTP server inspect correlates listener pressure with handler counters",
    "VM instrumentation correlates processes with mailboxes, reductions, resources, and native state",
    "distribution envelopes carry trace_id for multi-VM messages",
];

const SAMPLING_DECISIONS: &[&str] = &[
    "sampling policy remains rejected until production trace controls are implemented",
    "benchmark telemetry is always sampled for deterministic gate output",
    "debug/editor telemetry is read-only until operator mode is guarded",
];

const CARDINALITY_CHECKS: &[&str] = &[
    "route and handler names are bounded by compiled manifest entries",
    "request paths remain rejected as metric labels until route template labeling exists",
    "stream ids remain rejected as metric labels until bounded cardinality policy exists",
];

const SURFACE_MATRIX: &[&str] = &[
    "text serve logs",
    "JSON quality report",
    "support-bundle rejected path",
    "benchmark VM HTTP lane",
    "debugger VM diagnostics",
    "editor source-map surfaces",
];

const REJECTED_OBSERVABILITY_PATHS: &[&str] = &[
    "connection id emitted on every WebSocket/SSE exchange",
    "actor id emitted on every web handler exchange",
    "route id emitted as bounded metric label instead of raw path",
    "template stream id emitted for live-template streams",
    "typed security decision id carried by HTTP runtime",
    "typed config profile id carried by runtime telemetry",
    "support-bundle replay for web telemetry",
    "production trace sampling controls",
    "metric cardinality budget enforcement",
    "benchmark/support-bundle telemetry parity",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmWebObservabilitySummary {
    pub telemetry_field_count: usize,
    pub route_trace_count: usize,
    pub stream_trace_count: usize,
    pub rejected_observability_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_web_observability(root: &Path) -> QualityResult<VmWebObservabilitySummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/logging.rs",
        REQUIRED_SERVE_LOGGING_ANCHORS,
        "serve request observability",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/serve_test.rs",
        REQUIRED_SERVE_TEST_ANCHORS,
        "serve request observability tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http.rs",
        REQUIRED_VM_HTTP_ANCHORS,
        "VM HTTP observability counters",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_test.rs",
        REQUIRED_VM_HTTP_TEST_ANCHORS,
        "VM HTTP observability tests",
    )?);
    for (relative, anchors) in REQUIRED_STREAM_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM web stream observability",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/code_server.rs",
        REQUIRED_SOURCE_MAP_ANCHORS,
        "VM source-map observability",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/vm/instrumentation.rs",
        REQUIRED_VM_INSTRUMENTATION_ANCHORS,
        "VM instrumentation observability",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/benchmark/http_runtime_lane.rs",
        REQUIRED_BENCHMARK_ANCHORS,
        "VM HTTP benchmark observability",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries(
        "telemetry schema",
        TELEMETRY_SCHEMA,
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-web-observability", &diagnostics));
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
        "schema": "terlan-vm-web-observability-report-v1",
        "telemetrySchema": TELEMETRY_SCHEMA,
        "routeTraces": ROUTE_TRACES,
        "streamTraces": STREAM_TRACES,
        "securityDecisionTraces": SECURITY_DECISION_TRACES,
        "redactionChecks": REDACTION_CHECKS,
        "correlationChecks": CORRELATION_CHECKS,
        "samplingDecisions": SAMPLING_DECISIONS,
        "cardinalityChecks": CARDINALITY_CHECKS,
        "surfaceMatrix": SURFACE_MATRIX,
        "rejectedObservabilityPaths": REJECTED_OBSERVABILITY_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM web observability report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmWebObservabilitySummary {
        telemetry_field_count: TELEMETRY_SCHEMA.len(),
        route_trace_count: ROUTE_TRACES.len(),
        stream_trace_count: STREAM_TRACES.len(),
        rejected_observability_path_count: REJECTED_OBSERVABILITY_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read VM web observability gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM web observability gate term `{term}`"))
        .collect())
}

fn validate_no_placeholder_report_entries(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!(
                        "VM web observability {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_web_observability_test.rs"]
mod vm_web_observability_test;
