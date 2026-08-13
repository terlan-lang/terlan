use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::support::validate_required_terms;
use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-stateful-actor-session-report.json";

const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_RUNTIME_ANCHORS: &[&str] = &[
    "VmHttpSessionRuntime",
    "VmHttpSessionRecoveryPolicy",
    "VmHttpSessionRoute",
    "VmHttpSessionSnapshot",
    "VmHttpSessionAffinityKey",
    "VmHttpSessionAffinityError",
    "VmHttpSessionCommandOutcome",
    "VmHttpSessionPersistenceSnapshot",
    "VmHttpSessionMailboxBackpressure",
    "VmHttpSessionWorkerMigration",
    "VmHttpSessionHotReloadMigrationReport",
    "VmHttpSessionLiveTemplateSubscriber",
    "VmActorRuntime",
    "VmTableStore",
    "lookup_or_create",
    "lookup_or_create_with_affinity_keys",
    "resolve_http_session_affinity_key",
    "crashed_session_actor_diagnostic",
    "session_actor_exit_reason",
    "apply_idempotent_command",
    "state_version",
    "apply_state_update",
    "state_version_conflict_diagnostic",
    "persistence_snapshot",
    "replay_persistence_snapshot",
    "enqueue_actor_message",
    "actor_mailbox_backpressure",
    "migrate_to_worker",
    "worker_migration_diagnostic",
    "hot_reload_migration_compatibility_report",
    "hot_reload_migration_compatibility_diagnostic",
    "subscribe_live_template",
    "unsubscribe_live_template",
    "live_template_subscribers",
    "create_session",
    "lookup_existing",
    "rotate",
    "expire_due",
    "snapshots",
    "sticky_key",
    "std.http.Session",
    "current",
    "set",
    "get",
    "delete",
    "with_response",
];

const REQUIRED_TEST_ANCHORS: &[&str] = &[
    "http_session_lookup_creates_actor_and_sticky_metadata",
    "http_session_adapter_functions_delegate_to_actor_runtime",
    "http_session_adapter_renders_non_string_values_for_string_get",
    "http_session_blank_cookie_creates_replacement_session",
    "http_session_affinity_accepts_single_typed_key",
    "http_session_affinity_merges_duplicate_matching_keys",
    "http_session_affinity_rejects_missing_and_conflicting_keys",
    "http_session_table_event_adapters_are_defensive",
    "http_session_delete_reports_stale_table_after_internal_cleanup",
    "http_session_private_lookup_paths_report_stale_sessions",
    "http_session_reuses_actor_and_table_state_for_cookie_lookup",
    "http_session_actor_crash_during_request_cleans_state_and_replaces_cookie",
    "http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state",
    "http_session_idempotent_command_replays_duplicate_result_without_rerun",
    "http_session_live_template_subscribers_are_cleaned_after_actor_exit",
    "http_session_state_update_rejects_stale_concurrent_writer",
    "http_session_persistence_snapshot_replays_after_restart",
    "http_session_actor_mailbox_backpressure_is_attributed",
    "http_session_migrates_durable_state_across_workers",
    "http_session_reports_hot_reload_migration_compatibility",
    "http_session_rotate_changes_cookie_without_losing_actor_state",
    "http_session_expiration_cleans_actor_table_and_reports_stale",
    "http_session_recovery_policy_can_fail_closed_for_stale_cookie",
    "http_session_rejects_invalid_runtime_configuration",
];

const REQUIRED_EXACT_SELECTORS: &[&str] = &[];

const AFFINITY_FIXTURES: &[&str] = &[
    "missing session cookie creates actor-backed session",
    "existing cookie reuses actor and table state",
    "session rotation preserves actor state",
    "session expiration cleans actor table state",
    "fail-closed stale cookie recovery",
    "conflicting affinity key rejection",
    "invalid runtime configuration rejection",
    "source-level session descriptor lifecycle",
    "expired source-level session descriptor rejection",
    "actor crash during request cleanup and replacement",
    "reconnect after actor crash creates clean replacement",
    "duplicate command idempotency replay",
    "live-template subscriber cleanup after actor exit",
    "concurrent state update conflict rejection",
    "persistence hook replay after restart",
    "stateful actor mailbox backpressure attribution",
    "session migration across workers",
    "hot reload migration compatibility report",
];

const LIFECYCLE_TRACES: &[&str] = &[
    "spawn std.http.Session actor",
    "create owner-only VM table",
    "write typed VM value",
    "read typed VM value",
    "delete typed VM value",
    "rotate session id",
    "expire actor and table",
    "inspect live snapshots",
];

const REJECTED_SESSION_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm http stateful actor session summary.
pub struct VmHttpStatefulActorSessionSummary {
    pub affinity_fixture_count: usize,
    pub lifecycle_trace_count: usize,
    pub exact_selector_count: usize,
    pub rejected_session_path_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm http stateful actor session.
pub fn run_vm_http_stateful_actor_session(
    root: &Path,
) -> QualityResult<VmHttpStatefulActorSessionSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session.rs",
        REQUIRED_RUNTIME_ANCHORS,
        "VM HTTP stateful session runtime",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        REQUIRED_TEST_ANCHORS,
        "VM HTTP stateful session tests",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-http-stateful-actor-session",
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
        "schema": "terlan-vm-http-stateful-actor-session-report-v1",
        "affinityFixtures": AFFINITY_FIXTURES,
        "actorLifecycleTraces": LIFECYCLE_TRACES,
        "stateTransitionTraces": [
            "create",
            "lookup",
            "write",
            "read",
            "delete",
            "rotate",
            "expire",
            "recover",
            "migrate"
        ],
        "reconnectCases": {
            "implemented": true,
            "evidence": "http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state"
        },
        "duplicateCommandHandling": {
            "implemented": true,
            "evidence": "http_session_idempotent_command_replays_duplicate_result_without_rerun"
        },
        "liveTemplateSubscriberCleanup": {
            "implemented": true,
            "evidence": "http_session_live_template_subscribers_are_cleaned_after_actor_exit"
        },
        "concurrentStateUpdates": {
            "implemented": true,
            "evidence": "http_session_state_update_rejects_stale_concurrent_writer"
        },
        "persistenceHookReplay": {
            "implemented": true,
            "evidence": "http_session_persistence_snapshot_replays_after_restart"
        },
        "backpressureCases": {
            "implemented": true,
            "evidence": "http_session_actor_mailbox_backpressure_is_attributed"
        },
        "workerMigrationResults": {
            "implemented": true,
            "evidence": "http_session_migrates_durable_state_across_workers"
        },
        "hotReloadMigrationResults": {
            "implemented": true,
            "evidence": "http_session_reports_hot_reload_migration_compatibility"
        },
        "rejectedSessionPaths": REJECTED_SESSION_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM HTTP stateful session report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmHttpStatefulActorSessionSummary {
        affinity_fixture_count: AFFINITY_FIXTURES.len(),
        lifecycle_trace_count: LIFECYCLE_TRACES.len(),
        exact_selector_count: REQUIRED_EXACT_SELECTORS.len(),
        rejected_session_path_count: REJECTED_SESSION_PATHS.len(),
        report_path,
    })
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read VM HTTP stateful session gate: {err}"))?;
    let mut diagnostics = Vec::new();
    if !text
        .contains("vm-http-stateful-actor-session-check: vm-http-handler-scheduler-fairness-check")
    {
        diagnostics.push(
            "Makefile: VM HTTP stateful session gate must run after vm-http-handler-scheduler-fairness-check"
                .to_string(),
        );
    }
    if !text.contains("vm-http-stateful-actor-session") {
        diagnostics.push(
            "Makefile: VM HTTP stateful session gate must run terlan-quality vm-http-stateful-actor-session"
                .to_string(),
        );
    }
    Ok(diagnostics)
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "affinity fixtures",
        AFFINITY_FIXTURES,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "actor lifecycle traces",
        LIFECYCLE_TRACES,
    ));
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "rejected session paths",
        REJECTED_SESSION_PATHS,
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
                        "VM HTTP stateful actor session {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_http_stateful_actor_session_test.rs"]
#[cfg(test)]
mod vm_http_stateful_actor_session_test;
