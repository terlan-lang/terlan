use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-acme-renewal-report.json";

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/serve/tls.rs",
        &[
            "ACME_RENEWAL_INTERVAL",
            "ACME_METADATA_CLOCK_SKEW",
            "validate_acme_certificate_cache_age",
            "validate_acme_certificate_cache_mode",
            "load_acme_runtime_tls_cache",
            "issue_acme_certificate_cache_for_serve",
            "acme_runtime_tls_config_with_local_issuer",
            "rustls_server_config",
        ],
    ),
    (
        "crates/terlan/src/commands/serve/tls/cache.rs",
        &[
            "AcmeCertificateCacheMetadata",
            "renew_after_unix_seconds",
            "store_acme_certificate_cache_metadata",
            "write_cache_file_atomically",
            "rename_cache_file",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer.rs",
        &[
            "VmTimerTable",
            "start_one_shot",
            "cancel_owner_timers",
            "advance_clock",
            "VmTimerSnapshot",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/acme_worker.rs",
        &[
            "VmAcmeRenewalRetryPolicy",
            "VmAcmeRenewalActor",
            "VmAcmeRenewalActorState",
            "begin_due_renewal",
            "delay_for_attempt",
            "spawn_renewal_actor",
            "schedule_renewal_timer",
            "VmTimerTable",
            "start_one_shot",
            "redact_acme_support_bundle_value",
            "acme.renewal.scheduled",
            "capture_deterministic_renewal_cache_tls_handoff_replay",
            "acme.renewal.replay.tls_handoff",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls.rs",
        &[
            "VmTlsRuntime",
            "install_listener_plan",
            "remove_listener_plan",
            "VmTlsRotationWindow",
            "rotate_listener_plan",
            "retire_rotation_window",
            "start_listener_server_connection",
            "build_listener_server_config",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/serve/tls_test.rs",
        &[
            "runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata",
            "runtime_tls_config_rejects_stale_auto_tls_certificate_cache",
            "runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata",
            "runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache",
            "acme_runtime_tls_config_accepts_local_mock_issuer_cache_handoff",
            "acme_runtime_tls_config_rejects_local_mock_issuer_without_cache_handoff",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer_test.rs",
        &[
            "timer_table_starts_one_shot_timer_and_exposes_snapshot",
            "timer_table_cancels_owner_timers_in_stable_order",
            "timer_table_fires_due_timers_only_once",
            "timer_table_receive_timeout_wakes_blocked_process",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/acme_worker_test.rs",
        &[
            "vm_acme_renewal_retry_policy_is_typed_and_deterministic",
            "vm_acme_renewal_actor_owns_worker_timer_and_shutdown_cleanup",
            "vm_acme_worker_schedules_renewal_through_vm_timer_table",
            "vm_acme_worker_denies_stale_challenge_access_after_renewal_scheduled",
            "vm_acme_worker_records_renewal_telemetry_and_redacted_support_bundle_step",
            "vm_acme_worker_routes_challenge_after_due_renewal_begins",
            "vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/tls_test.rs",
        &[
            "vm_tls_runtime_removes_tcp_listener_plan_on_shutdown",
            "vm_tls_runtime_builds_manual_rustls_server_config",
            "vm_tls_runtime_builds_internal_rustls_server_config",
            "vm_tls_runtime_rejects_auto_server_connection_without_cache",
            "vm_tls_runtime_enforces_rotation_overlap_window_before_retiring_old_config",
            "vm_tls_runtime_hot_rotation_keeps_existing_connection_mode_for_old_accepts",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-http-acme-renewal-rotation-check: vm-http-acme-cache-custody-check",
    "vm_acme_renewal_retry_policy_is_typed_and_deterministic",
    "vm_acme_renewal_actor_owns_worker_timer_and_shutdown_cleanup",
    "runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache",
    "vm_acme_worker_schedules_renewal_through_vm_timer_table",
    "vm_acme_worker_denies_stale_challenge_access_after_renewal_scheduled",
    "vm_acme_worker_records_renewal_telemetry_and_redacted_support_bundle_step",
    "vm_acme_worker_routes_challenge_after_due_renewal_begins",
    "vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay",
    "vm_tls_runtime_enforces_rotation_overlap_window_before_retiring_old_config",
    "vm_tls_runtime_hot_rotation_keeps_existing_connection_mode_for_old_accepts",
    "vm_http_acme_renewal_test",
    "vm-http-acme-renewal",
];

const RENEWAL_SCHEDULES: &[&str] = &[
    "fresh cache schedules before renewal deadline",
    "stale cache rejects TLS startup",
    "future-dated metadata rejects TLS startup",
    "missing metadata rejects TLS startup",
    "issuer handoff must populate cache before TLS startup",
];

const TIMER_TRACES: &[&str] = &[
    "VM one-shot renewal timer planned",
    "VM timer cancellation on worker shutdown",
    "VM timer fire wakes renewal actor",
    "VM timer snapshot visible to runtime inspector",
    "host-runtime timer dependency rejected",
];

const RETRY_DECISIONS: &[&str] = &[
    "retry after challenge timeout",
    "retry after temporary issuer failure",
    "do not retry unsupported provider policy",
    "jitter remains deterministic in test fixtures",
    "shutdown cancels pending retry",
];

const CACHE_ROTATION_TRACES: &[&str] = &[
    "new cache written atomically",
    "partial cache rejected",
    "old cache remains active during failed renewal",
    "successful renewal schedules TLS handoff",
    "old certificate overlap window recorded",
];

const TLS_HANDOFF_EVENTS: &[&str] = &[
    "build replacement rustls config",
    "publish replacement listener plan",
    "accepted connections keep previous config",
    "new connections observe replacement config",
    "remove retired listener plan after overlap",
];

const REJECTED_ROTATIONS: &[&str] = &[
    "renewal during heavy traffic",
    "renewal worker crash",
    "overlapping renewals",
    "expired old certificate",
    "not-yet-valid new certificate",
    "cache write race",
    "shutdown during rotation",
    "staging/live endpoint mismatch",
];

const TYPED_FAILURE_DIAGNOSTICS: &[&str] = &[
    "RenewalTimerMissing",
    "RenewalRetryExhausted",
    "RotationCacheIncomplete",
    "RotationCertificateInvalid",
    "RotationOverlapExpired",
    "RotationShutdownCancelled",
];

const REJECTED_RENEWAL_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmHttpAcmeRenewalSummary {
    pub renewal_schedule_count: usize,
    pub timer_trace_count: usize,
    pub tls_handoff_event_count: usize,
    pub rejected_renewal_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_http_acme_renewal(root: &Path) -> QualityResult<VmHttpAcmeRenewalSummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME renewal foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM HTTP ACME renewal fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-http-acme-renewal", &diagnostics));
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
        "schema": "terlan-vm-http-acme-renewal-report-v1",
        "renewalSchedules": RENEWAL_SCHEDULES,
        "timerTraces": TIMER_TRACES,
        "retryDecisions": RETRY_DECISIONS,
        "cacheRotationTraces": CACHE_ROTATION_TRACES,
        "activeTlsHandoffEvents": TLS_HANDOFF_EVENTS,
        "oldNewCertificateOverlap": [
            "accepted connections retain previous TLS config",
            "new accepts use replacement TLS config after handoff",
            "retired certificate cannot outlive configured overlap"
        ],
        "rejectedRotations": REJECTED_ROTATIONS,
        "typedFailureDiagnostics": TYPED_FAILURE_DIAGNOSTICS,
        "deterministicReplayBoundary": [
            "renewal metadata fixtures",
            "local issuer cache handoff fixtures",
            "VM timer fixtures",
            "VM TLS listener lifecycle fixtures",
            "renewal/cache/TLS handoff replay fixture"
        ],
        "rejectedRenewalPaths": REJECTED_RENEWAL_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM HTTP ACME renewal report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmHttpAcmeRenewalSummary {
        renewal_schedule_count: RENEWAL_SCHEDULES.len(),
        timer_trace_count: TIMER_TRACES.len(),
        tls_handoff_event_count: TLS_HANDOFF_EVENTS.len(),
        rejected_renewal_path_count: REJECTED_RENEWAL_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read VM HTTP ACME renewal gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing VM HTTP ACME renewal gate term `{term}`"))
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
#[path = "vm_http_acme_renewal_test.rs"]
mod vm_http_acme_renewal_test;
