use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-acme-worker-report.json";

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/acme_worker.rs",
        &[
            "VmAcmeWorkerRuntime",
            "VmAcmeWorkerRequest",
            "VmAcmeHttp01Challenge",
            "VmAcmeWorkerState",
            "VmAcmeWorkerWake",
            "VmAcmeWorkerTelemetrySpan",
            "VmAcmeWorkerAccessDecision",
            "VmAcmeWorkerExecutionLane",
            "start_worker",
            "start_worker_for_lane",
            "validate_execution_lane",
            "with_owner_limit",
            "enforce_owner_backpressure",
            "telemetry_spans",
            "record_telemetry_span",
            "challenge_route_access_decision",
            "park_issuance_waiter",
            "VmAcmeWorkerWake::IssuanceReady",
            "VmAcmeWorkerWake::RenewalDue",
            "renewal_due_wakeups",
            "prepare_http01_challenge",
            "start_issuance",
            "begin_cache_write",
            "complete_worker",
            "schedule_renewal",
            "shutdown_owner_workers",
            "inspect_worker",
            "capture_support_bundle_step",
            "VmSupportBundleReplayResourceKind::AcmeWorker",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/tls/acme_runtime.rs",
        &[
            "instant_acme",
            "AcmeHttp01Challenge",
            "acme_http01_challenge",
            "runtime_tls_config_for_serve",
            "start_live_acme_worker_for_serve",
            "VmAcmeWorkerRuntime",
            "VmAcmeWorkerExecutionLane::Live",
            "VmProcessId::system_runtime_worker",
            "pending_http01_challenges",
            "rustls_server_config",
            "store_acme_http01_challenge",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls.rs",
        &[
            "VmTlsRuntime",
            "VmTlsMode::Auto",
            "build_listener_server_config",
            "start_listener_server_connection",
            "remove_listener_plan",
            "rustls_server_config",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/request_dispatch.rs",
        &[
            "acme_http01_challenge",
            "AcmeHttp01Challenge::Found",
            "AcmeHttp01Challenge::Missing",
            "AcmeHttp01Challenge::Invalid",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/acme_worker_test.rs",
        &[
            "vm_acme_worker_runs_http01_state_machine_without_network",
            "vm_acme_worker_rejects_invalid_inputs_and_cleans_up_owner_workers",
            "vm_acme_worker_captures_support_bundle_replay_steps",
            "vm_acme_worker_enforces_owner_backpressure_limit",
            "vm_acme_worker_emits_challenge_and_issuance_telemetry_spans",
            "vm_acme_worker_authorizes_http01_challenge_route_through_policy_hook",
            "vm_acme_worker_parks_and_wakes_issuance_waiters",
            "vm_acme_worker_emits_due_renewal_wakeups",
            "vm_acme_worker_uses_one_contract_for_fixture_and_live_lanes",
            "vm_acme_worker_starts_issuance_without_new_challenge_for_valid_authorizations",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/tls/acme_runtime/tls_test.rs",
        &[
            "serve_live_acme_issuance_starts_vm_worker_lane",
            "pending_http01_challenges_reject_missing_http01",
            "acme_http01_challenge_cache_writes_valid_token",
            "acme_http01_challenge_cache_rejects_invalid_token",
            "acme_certificate_cache_write_feeds_runtime_tls_config",
            "runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata",
            "runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata",
            "runtime_tls_config_rejects_stale_auto_tls_certificate_cache",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/serve_test.rs",
        &[
            "hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache",
            "hyper_request_handler_serves_acme_http01_head_without_body",
            "hyper_request_handler_returns_404_for_missing_acme_http01_challenge",
            "hyper_request_handler_rejects_invalid_acme_http01_token",
            "vm_stream_request_serves_acme_http01_challenge_without_hyper",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls_test.rs",
        &[
            "vm_tls_runtime_rejects_auto_server_connection_without_cache",
            "vm_tls_runtime_removes_tcp_listener_plan_on_shutdown",
            "vm_tls_runtime_builds_manual_rustls_server_config",
            "vm_tls_runtime_builds_internal_rustls_server_config",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-http-acme-tls-base-check: vm-timer-deadline-check http-tls-check",
    "vm-http-acme-worker-migration-check: vm-http-acme-tls-base-check",
    "vm-http-acme-worker",
];

const WORKER_STATE_TRACES: &[&str] = &[
    "request accepted",
    "challenge prepared",
    "issuance started",
    "cache write attempted",
    "renewal decision recorded",
    "worker cancellation observed",
    "worker shutdown observed",
    "VM-owned ACME worker state machine validated",
    "support bundle captures VM worker state",
    "owner-scoped issuance backpressure enforced",
    "challenge and issuance telemetry spans emitted",
    "issuance waiters park and wake through VM scheduler handles",
    "deterministic and live lanes share one VM worker contract",
    "serve auto TLS starts a VM-owned live ACME worker lane",
];

const CHALLENGE_ROUTING_TRACES: &[&str] = &[
    "HTTP-01 route reserved before static fallback",
    "HTTP-01 route collision rejected",
    "challenge response visible to VM HTTP router",
    "VM access-policy hook validates challenge route",
    "middleware receives ACME route metadata",
    "support bundle captures challenge lookup outcome",
];

const CACHE_PROVENANCE_FIELDS: &[&str] = &[
    "domain",
    "issuer",
    "account id",
    "challenge method",
    "cache format",
    "worker identity",
    "staging flag",
    "renewal timestamp",
];

const RENEWAL_DECISIONS: &[&str] = &[
    "fresh cache reused",
    "stale cache rejected",
    "future-dated cache rejected",
    "malformed cache rejected",
    "renewal race must serialize through worker",
    "due renewal emits VM wakeup",
];

const CANCELLATION_SHUTDOWN_OUTCOMES: &[&str] = &[
    "cancel before challenge cleanup",
    "cancel during issuance ignores stale native result",
    "shutdown waits for cache handoff boundary",
    "shutdown rejects new issuance requests",
    "shutdown records incomplete renewal",
];

const TYPED_DIAGNOSTIC_FIXTURES: &[&str] = &[
    "challenge timeout",
    "challenge route collision",
    "cache write failure",
    "renewal race",
    "worker cancellation",
    "shutdown during issuance",
    "staging/live confusion",
    "stale cache provenance",
];

const REJECTED_WORKER_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm http acme worker summary.
pub struct VmHttpAcmeWorkerSummary {
    pub worker_state_trace_count: usize,
    pub challenge_routing_trace_count: usize,
    pub typed_diagnostic_fixture_count: usize,
    pub rejected_worker_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm http acme worker.
pub fn run_vm_http_acme_worker(root: &Path) -> QualityResult<VmHttpAcmeWorkerSummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME worker foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME worker fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-http-acme-worker", &diagnostics));
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
        "schema": "terlan-vm-http-acme-worker-report-v1",
        "workerStateTraces": WORKER_STATE_TRACES,
        "challengeRoutingTraces": CHALLENGE_ROUTING_TRACES,
        "cacheProvenance": CACHE_PROVENANCE_FIELDS,
        "renewalDecisions": RENEWAL_DECISIONS,
        "cancellationShutdownOutcomes": CANCELLATION_SHUTDOWN_OUTCOMES,
        "stagingModeDocs": [
            "live issuance remains opt-in until the VM worker owns the lifecycle",
            "staging/live provider choice must be recorded in cache provenance",
            "ZeroSSL fallback remains rejected before network issuance"
        ],
        "typedDiagnosticFixtures": TYPED_DIAGNOSTIC_FIXTURES,
        "maintainedCrateBoundaries": [
            "instant_acme owns ACME protocol operations",
            "rustls owns TLS protocol state",
            "rustls-pemfile owns certificate/key PEM parsing",
            "rcgen owns local certificate/key generation",
            "serde_json owns cache metadata serialization"
        ],
        "rejectedWorkerPaths": REJECTED_WORKER_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM HTTP ACME worker report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmHttpAcmeWorkerSummary {
        worker_state_trace_count: WORKER_STATE_TRACES.len(),
        challenge_routing_trace_count: CHALLENGE_ROUTING_TRACES.len(),
        typed_diagnostic_fixture_count: TYPED_DIAGNOSTIC_FIXTURES.len(),
        rejected_worker_path_count: REJECTED_WORKER_PATHS.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = read_split_source(root, relative)
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

pub(super) fn read_split_source(root: &Path, relative: &str) -> Result<String, std::io::Error> {
    let path = root.join(relative);
    let mut text = fs::read_to_string(&path)?;
    let directory = path.with_extension("");
    append_rs_sources(&directory, &mut text)?;
    Ok(text)
}

fn append_rs_sources(directory: &Path, text: &mut String) -> Result<(), std::io::Error> {
    if !directory.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            append_rs_sources(&path, text)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            text.push('\n');
            text.push_str(&fs::read_to_string(path)?);
        }
    }
    Ok(())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM HTTP ACME worker gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM HTTP ACME worker gate term `{term}`"))
        .collect())
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
#[path = "vm_http_acme_worker_test.rs"]
#[cfg(test)]
mod vm_http_acme_worker_test;
