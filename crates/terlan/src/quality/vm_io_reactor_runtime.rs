use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-io-reactor-runtime-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_TCP_ANCHORS: &[&str] = &[
    "VmTcpRuntime",
    "VmTcpWake",
    "connect_with_wakeups",
    "send_with_wakeups",
    "receive_with_wakeups",
    "park_accept",
    "park_receive",
    "park_send",
    "close_write",
    "cancel_stream",
    "close_owner_streams",
    "inspect_stream",
    "inspect_listener",
];

const REQUIRED_TCP_SCHEDULER_ANCHORS: &[&str] = &[
    "VmTcpWakeReport",
    "apply_tcp_wakeups",
    "wake_process",
    "accept_wakeups",
    "read_wakeups",
    "write_wakeups",
    "diagnostics",
];

const REQUIRED_IO_REACTOR_ANCHORS: &[&str] = &[
    "VmIoReactorLoop",
    "VmIoReactorWake",
    "VmIoReactorDrain",
    "enqueue_tcp_wake",
    "enqueue_udp_wake",
    "enqueue_package_download_wake",
    "enqueue_debugger_wake",
    "enqueue_acme_worker_wake",
    "enqueue_timer_event",
    "drain_ready",
    "deterministic_trace",
    "wake_process",
];

const REQUIRED_IO_RUNTIME_BOUNDARY_ANCHORS: &[&str] = &[
    "VmExternalIoRuntimeBoundary",
    "VmExternalIoRuntimePlan",
    "VmExternalIoSchedulingPolicy",
    "VmWakeProducerOnly",
    "OwnsActorScheduling",
    "OwnsProcessContinuations",
    "DirectSchedulerAccess",
    "emits_typed_vm_wakeups",
    "enforces_bounded_backpressure",
    "records_support_bundle_replay",
    "validate",
];

const REQUIRED_UDP_ANCHORS: &[&str] = &[
    "VmUdpRuntime",
    "VmUdpSocket",
    "VmUdpWake",
    "bind_with_inbox_limit",
    "send_to_with_wakeups",
    "park_receive",
    "receive_from",
    "cancel_owner_sockets",
    "inspect_socket",
];

const REQUIRED_PACKAGE_DOWNLOAD_ANCHORS: &[&str] = &[
    "VmPackageDownloadRuntime",
    "VmPackageDownload",
    "VmPackageDownloadWake",
    "VmPackageDownloadEvent",
    "start_download",
    "enqueue_chunk",
    "finish_download",
    "park_receive",
    "receive_next",
    "cancel_owner_downloads",
    "inspect_download",
];

const REQUIRED_SUPPORT_BUNDLE_ANCHORS: &[&str] = &[
    "VmSupportBundleReplayMetadata",
    "VmSupportBundleReplayStep",
    "VmSupportBundleReplayRecorder",
    "VmSupportBundleReplayResource",
    "VmSupportBundleReplayExpectation",
    "record_io_step",
    "record_io_step_with_source",
    "finish_bundle",
    "replay_steps_after",
    "verify_replay_step",
];

const REQUIRED_IO_DIAGNOSTICS_ANCHORS: &[&str] = &[
    "VmIoDiagnostic",
    "VmIoDiagnosticLog",
    "VmIoDiagnosticSourceMap",
    "VmIoDiagnosticResource",
    "VmIoDiagnosticSeverity",
    "source_map_id",
    "record_diagnostic",
    "diagnostics_for_source_map",
    "render_source_map_location",
    "render_text",
];

const REQUIRED_DEBUGGER_TRANSPORT_ANCHORS: &[&str] = &[
    "VmDebuggerTransportRuntime",
    "VmDebuggerSession",
    "VmDebuggerCommand",
    "VmDebuggerEvent",
    "VmDebuggerWake",
    "open_session",
    "enqueue_command",
    "park_command_receive",
    "receive_command",
    "enqueue_event",
    "park_event_receive",
    "receive_event",
    "close_owner_sessions",
    "inspect_session",
];

const REQUIRED_TIMER_ANCHORS: &[&str] = &[
    "VmTimerTable",
    "start_receive_timeout",
    "advance_clock",
    "wake_process",
    "cancel_owner_timers",
    "VmTimerEvent",
    "VmTimerSnapshot",
];

const REQUIRED_FRAMING_ANCHORS: &[&str] = &[
    "VmInMemoryFrameReader",
    "read_exact_with_timeout",
    "VmFramingError",
    "BackpressureExceeded",
    "Timeout",
    "Cancelled",
    "FramingEof",
];

const REQUIRED_HTTP_ANCHORS: &[&str] = &[
    "VmHttpTcpServer",
    "poll_or_park_http1_tcp_exchange",
    "poll_or_park_http1_tls_tcp_exchange_with_connection",
    "accept_http1_tcp_handler",
    "finish_http1_tcp_handler",
    "cancel_handler",
    "shutdown",
    "tcp.park_receive",
    "actor.block",
    "actor.wake",
];

const REQUIRED_STREAM_PROTOCOL_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/sse.rs",
        &[
            "VmSseStream",
            "VmSseEndpointPlan",
            "BackpressureExceeded",
            "close",
            "inspect",
            "keep_alive_ms",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/websocket.rs",
        &[
            "VmWebSocketRuntime",
            "VmWebSocketInboundQueue",
            "accept_upgrade",
            "terminate_session_and_stream",
            "VmWebSocketTerminationReason",
            "Cancelled",
            "close_all_sessions",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls.rs",
        &[
            "VmTlsRuntime",
            "VmTlsTcpServerStream",
            "VmTlsTcpPoll",
            "NeedRead",
            "Handshaking",
            "Ready",
            "listener_transport_mode",
            "remove_listener_plan",
        ],
    ),
];

const REQUIRED_ACME_WORKER_ANCHORS: &[&str] = &[
    "VmAcmeWorkerRuntime",
    "VmAcmeWorkerExecutionLane",
    "Live",
    "start_worker_for_lane",
    "park_issuance_waiter",
    "start_issuance",
    "VmAcmeWorkerWake",
    "ChallengeReady",
    "IssuanceReady",
    "RenewalDue",
    "schedule_renewal_timer",
    "spawn_renewal_actor",
    "capture_support_bundle_step",
    "capture_deterministic_renewal_cache_tls_handoff_replay",
];

const REQUIRED_NO_EXTERNAL_RUNTIME_TERMS: &[&str] = &[
    "VM-owned runtime paths must not depend on Tokio",
    "unexpected direct Tokio dependency",
];

const REQUIRED_EXACT_SELECTORS: &[&str] = &[
    "runtime::vm::tcp::tcp_test::tcp_runtime_parks_accept_and_reports_wakeup_when_connection_arrives",
    "runtime::vm::tcp::tcp_test::tcp_runtime_parks_receive_and_reports_wakeup_when_bytes_arrive",
    "runtime::vm::tcp::tcp_test::tcp_runtime_parks_send_and_reports_wakeup_when_peer_drains_capacity",
    "runtime::vm::udp::udp_test::udp_runtime_delivers_packet_bursts_and_wakes_receiver",
    "runtime::vm::udp::udp_test::udp_runtime_enforces_backpressure_and_cancels_owner_sockets",
    "runtime::vm::package_transport::package_transport_test::package_download_transport_parks_and_wakes_when_chunk_arrives",
    "runtime::vm::package_transport::package_transport_test::package_download_transport_enforces_backpressure_and_cancels_owner",
    "runtime::vm::support_bundle::support_bundle_test::support_bundle_replay_metadata_records_ordered_io_steps",
    "runtime::vm::support_bundle::support_bundle_test::support_bundle_replay_metadata_rejects_mismatched_replay_identity",
    "runtime::vm::io_diagnostics::io_diagnostics_test::io_diagnostics_render_source_map_aware_runtime_failures",
    "runtime::vm::io_diagnostics::io_diagnostics_test::io_diagnostics_reject_malformed_source_map_context",
    "runtime::vm::debugger_transport::debugger_transport_test::debugger_transport_parks_and_wakes_command_and_event_receivers",
    "runtime::vm::debugger_transport::debugger_transport_test::debugger_transport_enforces_backpressure_and_closes_owner_sessions",
    "runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_accept_process",
    "runtime::vm::io_reactor::io_reactor_test::vm_io_reactor_loop_drains_mixed_wakeups_in_deterministic_order",
    "runtime::vm::io_reactor::io_reactor_test::vm_io_reactor_loop_reports_stale_processes_without_stopping_later_wakeups",
    "runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_accepts_vm_wakeup_only_byte_producer",
    "runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_rejects_scheduling_and_hidden_continuations",
    "runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_rejects_unreplayable_or_unbounded_helpers",
    "runtime::vm::timer::timer_test::timer_table_receive_timeout_wakes_blocked_process",
    "runtime::vm::framing::framing_test::vm_framing_fixture_reports_timeout_for_pending_exact_read",
    "runtime::vm::framing::framing_test::vm_framing_fixture_reports_cancelled_streams",
    "runtime::vm::framing::framing_test::vm_framing_fixture_reports_backpressure_from_peer_inbox",
    "runtime::vm::http::http_test::vm_http_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter",
    "runtime::vm::http::http_test::vm_http_tcp_server_cancels_parked_handler_and_closes_stream",
    "runtime::vm::sse::sse_test::vm_sse_stream_enforces_backpressure_and_event_size",
    "runtime::vm::websocket::websocket_test::vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame",
    "runtime::vm::tls::tls_test::vm_tls_runtime_starts_manual_server_connection_with_readiness_state",
    "runtime::vm::tls::tls_test::vm_tls_tcp_server_stream_roundtrips_over_vm_tcp_runtime",
    "runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_uses_one_contract_for_fixture_and_live_lanes",
    "runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_routes_challenge_after_due_renewal_begins",
    "runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay",
];

const REACTOR_FIXTURES: &[&str] = &[
    "TCP listener/stream readiness",
    "UDP packet readiness",
    "package download chunk readiness",
    "support-bundle replay metadata",
    "source-map aware I/O diagnostics",
    "debugger transport readiness",
    "TCP scheduler wake adapter",
    "single unified I/O reactor loop",
    "external async runtime scheduling boundary",
    "receive timeout timers",
    "bounded in-memory framing",
    "HTTP actor poll parking",
    "SSE bounded stream queues",
    "WebSocket session termination",
    "TLS/TCP readiness state",
    "ACME live worker reactor integration",
];

const REJECTED_RUNTIME_PATHS: &[&str] = &[];

/// Summary produced by the VM I/O reactor runtime gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmIoReactorRuntimeSummary {
    pub fixture_count: usize,
    pub exact_selector_count: usize,
    pub rejected_runtime_count: usize,
    pub report_path: PathBuf,
}

/// Runs the VM I/O reactor runtime quality check.
///
/// Inputs:
/// - `root`: repository root with VM TCP/timer/framing/HTTP/SSE/WebSocket/TLS
///   runtime code, no-external-runtime checks, and exact Make gate selectors.
///
/// Output:
/// - Success summary and a report when current VM-owned I/O reactor behavior is
///   explicit and gated.
/// - Stable diagnostics when readiness, wakeup, timer, backpressure,
///   cancellation, or no-external-runtime guarantees drift.
///
/// Transformation:
/// - Freezes the current I/O baseline around VM-owned TCP, UDP, package
///   downloads, support-bundle replay metadata, source-map-aware diagnostics,
///   debugger transport, unified reactor scheduling, external helper boundary
///   validation, timers, stream protocols, ACME live worker integration, and
///   scheduler wakeups without retaining rejected runtime paths in the report.
pub fn run_vm_io_reactor_runtime(root: &Path) -> QualityResult<VmIoReactorRuntimeSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/tcp.rs",
        REQUIRED_TCP_ANCHORS,
        "VM TCP reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/tcp_scheduler.rs",
        REQUIRED_TCP_SCHEDULER_ANCHORS,
        "VM TCP scheduler adapter",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/io_reactor.rs",
        REQUIRED_IO_REACTOR_ANCHORS,
        "VM unified I/O reactor loop",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/io_runtime_boundary.rs",
        REQUIRED_IO_RUNTIME_BOUNDARY_ANCHORS,
        "VM external I/O runtime boundary",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/udp.rs",
        REQUIRED_UDP_ANCHORS,
        "VM UDP packet reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/package_transport.rs",
        REQUIRED_PACKAGE_DOWNLOAD_ANCHORS,
        "VM package download transport reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/support_bundle.rs",
        REQUIRED_SUPPORT_BUNDLE_ANCHORS,
        "VM support-bundle replay metadata",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/io_diagnostics.rs",
        REQUIRED_IO_DIAGNOSTICS_ANCHORS,
        "VM source-map-aware I/O diagnostics",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/debugger_transport.rs",
        REQUIRED_DEBUGGER_TRANSPORT_ANCHORS,
        "VM debugger transport reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/timer.rs",
        REQUIRED_TIMER_ANCHORS,
        "VM timer reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/framing.rs",
        REQUIRED_FRAMING_ANCHORS,
        "VM framing reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http.rs",
        REQUIRED_HTTP_ANCHORS,
        "VM HTTP reactor",
    )?);
    for (relative, terms) in REQUIRED_STREAM_PROTOCOL_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            terms,
            "VM stream protocol reactor",
        )?);
    }
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/acme_worker.rs",
        REQUIRED_ACME_WORKER_ANCHORS,
        "VM ACME live worker reactor",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/quality/no_default_tokio_runtime.rs",
        REQUIRED_NO_EXTERNAL_RUNTIME_TERMS,
        "no external runtime gate",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;

    Ok(VmIoReactorRuntimeSummary {
        fixture_count: REACTOR_FIXTURES.len(),
        exact_selector_count: REQUIRED_EXACT_SELECTORS.len(),
        rejected_runtime_count: REJECTED_RUNTIME_PATHS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    required_terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = read_repo_text_with_includes(root, relative)?;
    let normalized = normalize_text(&text);
    Ok(required_terms
        .iter()
        .filter(|term| !normalized.contains(&normalize_text(term)))
        .map(|term| format!("{relative}: missing {label} term `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read_repo_text(root, "Makefile")?;
    let mut diagnostics = Vec::new();
    if !makefile.contains("vm-io-reactor-runtime-check:") {
        diagnostics.push("Makefile: missing `vm-io-reactor-runtime-check` target".to_string());
    }
    if !makefile.contains("vm-native-worker-runtime-check") {
        diagnostics.push(
            "Makefile: VM I/O reactor gate must depend on VM native worker runtime".to_string(),
        );
    }
    if !makefile.contains("no-default-tokio-runtime-check") {
        diagnostics.push(
            "Makefile: VM I/O reactor gate must run no-default-Tokio runtime ownership check"
                .to_string(),
        );
    }
    if !makefile.contains("terlan-quality --quiet -- vm-io-reactor-runtime") {
        diagnostics.push(
            "Makefile: missing `terlan-quality --quiet -- vm-io-reactor-runtime` invocation"
                .to_string(),
        );
    }
    for selector in REQUIRED_EXACT_SELECTORS {
        if !makefile.contains(selector) {
            diagnostics.push(format!(
                "Makefile: missing VM I/O reactor exact selector `{selector}`"
            ));
        }
    }
    Ok(diagnostics)
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "reactor fixtures",
        REACTOR_FIXTURES,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "rejected runtime paths",
        REJECTED_RUNTIME_PATHS,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "exact selectors",
        REQUIRED_EXACT_SELECTORS,
    ));
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
                        "VM I/O reactor runtime {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
}

fn write_report(path: &Path) -> QualityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-io-reactor-runtime-report-v1",
        "baseline": "VM-owned TCP, UDP, package download, support-bundle replay, source-map-aware diagnostics, debugger transport, unified I/O reactor loop, external I/O runtime boundary, timer, framing, HTTP, SSE, WebSocket, TLS, and ACME live worker readiness fixtures",
        "reactorFixtures": REACTOR_FIXTURES,
        "wakeupTraces": [
            "TCP accept readiness emits VmTcpWake::Accept",
            "TCP read readiness emits VmTcpWake::Read",
            "TCP write capacity emits VmTcpWake::Write",
            "UDP packet arrival emits VmUdpWake::Receive",
            "package chunk arrival emits VmPackageDownloadWake::Chunk",
            "package download completion emits VmPackageDownloadWake::Complete",
            "support bundle replay metadata records deterministic I/O readiness",
            "source-map-aware I/O diagnostics retain resource and source identity",
            "debugger command/event readiness emits VmDebuggerWake",
            "apply_tcp_wakeups hands readiness to VmScheduler",
            "VmIoReactorLoop drains mixed readiness through one deterministic VM scheduler path",
            "stale reactor wakeups produce diagnostics without stopping later wakeups",
            "external I/O helpers must emit typed VM wakeups instead of owning actor scheduling",
            "ACME challenge, issuance, cache, renewal, and terminal states emit VmAcmeWorkerWake"
        ],
        "timerTraces": [
            "receive timeout blocks process",
            "advance_clock wakes receive-timeout owner",
            "timer cancellation emits stable events",
            "owner cleanup cancels timers deterministically"
        ],
        "socketLifecycleTraces": [
            "listener backlog and waiting acceptors are inspectable",
            "stream inbox pressure and waiting readers/writers are inspectable",
            "UDP packet inbox pressure and waiting receivers are inspectable",
            "package download chunk pressure and waiting receivers are inspectable",
            "support-bundle replay resources and outcomes are inspectable",
            "half-close blocks sender while peer replies remain possible",
            "owner stream cleanup closes actor-owned streams"
        ],
        "cancellationCases": [
            "TCP stream cancellation",
            "framing cancellation",
            "HTTP parked handler cancellation",
            "WebSocket cancelled termination"
        ],
        "supportBundleReplayMetadata": [
            "scheduler seed is captured",
            "I/O steps use monotonic one-based sequences",
            "process id, resource kind, handle, operation, outcome, and optional source identity are replayed",
            "mismatched replay identities are rejected"
        ],
        "sourceMapAwareIoDiagnostics": [
            "source_map_id is mandatory",
            "source file, module, function, and one-based span are captured",
            "diagnostics retain resource kind, handle, operation, severity, code, and message",
            "diagnostics can be filtered by source_map_id"
        ],
        "debuggerTransport": [
            "debugger sessions use VM-owned typed handles",
            "command and event queues are bounded",
            "blocked command and event receivers wake through VmDebuggerWake",
            "owner cleanup closes sessions and makes handles stale"
        ],
        "backpressureCases": [
            "TCP backlog limit",
            "TCP inbox limit",
            "UDP packet inbox limit",
            "package download chunk queue limit",
            "debugger command and event queue limits",
            "in-memory framing buffer limit",
            "SSE pending event and event-size limits",
            "HTTP accept and handler poll limits",
            "WebSocket inbound queue pressure"
        ],
        "acmeLiveWorkerReactor": [
            "fixture and live lanes share one typed VM worker contract",
            "live lane requires an HTTPS ACME directory URL",
            "HTTP-01 challenge readiness is exposed as VM wakeups",
            "issuance waiters park and resume through VmAcmeWorkerWake",
            "renewal scheduling uses VmTimerTable ownership",
            "support-bundle replay redacts account and cache secrets",
            "deterministic renewal cache/TLS handoff replay preserves old connections"
        ],
        "resourceCleanup": [
            "HTTP handler finish closes VM TCP stream",
            "HTTP server shutdown closes listener and handlers",
            "TLS listener shutdown removes listener plan",
            "WebSocket session close can close bound VM TCP streams",
            "ACME renewal actor shutdown releases VM timer and process resource ownership"
        ],
        "noExternalRuntimeOwnershipChecks": [
            "no-default-tokio-runtime-check remains in the gate",
            "VM runtime paths are checked against Tokio scheduling ownership",
            "external I/O helpers are validated as VM wake producers only",
            "actor scheduling, process continuations, and direct scheduler access are rejected"
        ],
        "rejectedRuntimePaths": REJECTED_RUNTIME_PATHS,
        "exactSelectors": REQUIRED_EXACT_SELECTORS
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM I/O reactor runtime report: {err}"))?;
    fs::write(path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write VM I/O reactor runtime report: {err}",
            path.display()
        )
    })
}

fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|err| format!("{relative}: failed to read file: {err}"))
}

fn read_repo_text_with_includes(root: &Path, relative: &str) -> QualityResult<String> {
    fn append_file(
        root: &Path,
        relative: &Path,
        visited: &mut BTreeSet<PathBuf>,
        output: &mut String,
    ) -> QualityResult<()> {
        if !visited.insert(relative.to_path_buf()) {
            return Ok(());
        }
        let display = relative.to_string_lossy();
        let text = read_repo_text(root, &display)?;
        output.push_str(&text);
        output.push('\n');

        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let mut remaining = text.as_str();
        while let Some(start) = remaining.find("include!(\"") {
            let after_start = &remaining[start + "include!(\"".len()..];
            let Some(end) = after_start.find("\")") else {
                break;
            };
            append_file(root, &parent.join(&after_start[..end]), visited, output)?;
            remaining = &after_start[end + 2..];
        }

        let mut remaining = text.as_str();
        while let Some(start) = remaining.find("#[path = \"") {
            let after_start = &remaining[start + "#[path = \"".len()..];
            let Some(end) = after_start.find("\"]") else {
                break;
            };
            append_file(root, &parent.join(&after_start[..end]), visited, output)?;
            remaining = &after_start[end + 2..];
        }
        Ok(())
    }

    let mut output = String::new();
    append_file(root, Path::new(relative), &mut BTreeSet::new(), &mut output)?;
    Ok(output)
}

fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-io-reactor-runtime] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_io_reactor_runtime_test.rs"]
mod vm_io_reactor_runtime_test;
