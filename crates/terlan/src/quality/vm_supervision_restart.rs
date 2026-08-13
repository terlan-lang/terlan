use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-supervision-report.json";

const REQUIRED_RUNTIME_ANCHORS: &[&str] = &[
    "VmSupervisionSystem",
    "VmSupervisorSnapshot",
    "VmSupervisorRestartHistoryEntry",
    "VmSupervisorRestartHistoryOutcome",
    "VmSupervisorState",
    "VmSupervisionRestart",
    "VmRestartPolicy",
    "VmChildRestartClass",
    "VmRestartBackoffSchedule",
    "VmShutdownTimeout",
    "VmChildSpec",
    "create_supervisor",
    "create_child_supervisor_with_policy",
    "start_child",
    "restart_child",
    "snapshot",
    "LimitReached",
    "Restarted",
    "RestartedGroup",
    "NotRestarted",
    "RestForOne",
    "Permanent",
    "Transient",
    "Temporary",
    "last_restart_delay_ms",
    "restart_delay_ms",
    "shutdown_timeout_ms",
    "last_shutdown_timeout_ms",
    "restart_history",
    "Failed",
    "ChildSupervisorFailed",
    "parent_id",
];

const REQUIRED_SHUTDOWN_ANCHORS: &[&str] = &[
    "VmSupervisionShutdownQueue",
    "VmScheduledSupervisionShutdown",
    "VmSupervisionShutdownStart",
    "VmSupervisionShutdownCompletion",
    "begin_shutdown",
    "complete_shutdown",
    "advance_clock",
    "handle_timer_event",
    "ShutdownTimeout",
];

const REQUIRED_PRODUCT_RUNTIME_ANCHORS: &[&str] = &[
    "VmSupervisionRuntime",
    "VmSupervisionChildSpec",
    "restart_failed_supervisor",
    "schedule_restart",
    "advance_restart_clock",
    "begin_shutdown",
    "advance_shutdown_clock",
    "charge_child_memory",
    "pending_lifecycle_count",
];

const REQUIRED_TEST_SELECTORS: &[&str] = &[
    "runtime::vm::actor::tests::actor_relationship_test::actor_unlinked_child_termination_preserves_parent_mailbox_progress",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_starts_child_and_exposes_inspection_snapshot",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_only_failed_child_for_one_for_one_policy",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_all_children_for_one_for_all_policy",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_one_for_all_enforces_restart_limit_before_group_restart",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_failed_and_later_children_for_rest_for_one_policy",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_rest_for_one_enforces_restart_limit_before_group_restart",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_temporary_child_never_restarts",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_transient_child_restarts_only_after_abnormal_exit",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_skips_non_restartable_children_without_blocking_restartable_siblings",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_applies_exponential_restart_backoff_for_one_for_one_policy",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_reports_per_child_backoff_delays",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_records_shutdown_timeout_for_live_child_restart",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_reports_per_child_shutdown_timeouts",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_enforces_restart_limit",
    "runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_records_supervisor_failure_when_restart_limit_escalates",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_propagates_child_supervisor_failure_to_parent_snapshot",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_records_restart_history_for_restart_and_limit",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_records_restart_history_for_non_restartable_child",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_child_diagnostic",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_supervisor_diagnostic",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_rejects_duplicate_child_id",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_restart_exits_live_child_before_restarting",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_process_instead_of_panicking_on_restart",
    "runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_supervisor_for_restart_and_snapshot",
    "runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_waits_for_clean_exit_and_cancels_deadline",
    "runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination",
    "runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_deadline_forces_typed_exit_and_restarts_child",
    "runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_normal_exit_honors_transient_restart_class",
    "runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically",
    "product_parent_strategy_restarts_failed_and_sibling_supervisor_subtrees",
    "product_native_boundary_worker_crash_uses_vm_backoff_and_restart",
    "product_handler_pool_memory_exhaustion_restarts_the_group",
    "product_in_flight_shutdown_timeout_cancels_old_actor_and_restarts",
];

const SUPERVISION_FIXTURES: &[&str] = &[
    "unlinked child termination preserves former supervisor mailbox progress",
    "supervisor creation and inspection snapshot",
    "child start with source metadata",
    "one-for-one restart replaces only failed child",
    "one-for-all restart replaces every child",
    "one-for-all restart limit prevents partial group restart",
    "rest-for-one restart replaces failed and later children",
    "rest-for-one restart limit prevents partial group restart",
    "temporary child never restarts",
    "transient child restarts only after abnormal exit",
    "group restart skips non-restartable children without blocking restartable siblings",
    "one-for-one restart backoff schedule",
    "group restart per-child backoff delay reporting",
    "live child restart records shutdown timeout",
    "group restart per-child shutdown timeout reporting",
    "restart exits live child before replacement",
    "restart intensity limit returns terminal outcome",
    "restart intensity exhaustion records supervisor failure",
    "parent supervisor observes child supervisor terminal failure",
    "restart history records successful and terminal outcomes",
    "restart history records non-restart outcomes",
    "duplicate child id rejection",
    "missing child diagnostic",
    "missing supervisor diagnostic",
    "missing process diagnostic without panic",
    "snapshot remains available after failed restart",
    "cooperative child shutdown cancels its VM timer deadline",
    "shutdown deadline forces a typed timeout exit and child replacement",
    "transient child clean shutdown remains non-restarting",
    "duplicate and overflowing shutdown deadlines are rejected atomically",
    "parent supervisor strategy rebuilds selected subtrees",
    "NativeBoundary worker crash uses VM backoff",
    "handler pool exhaustion restarts the selected group",
    "in-flight shutdown timeout cancels and replaces the old actor",
];

const OPEN_RESTART_GAPS: &[&str] = &[];

/// Summary produced by the VM supervision restart gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSupervisionRestartSummary {
    pub fixture_count: usize,
    pub exact_selector_count: usize,
    pub open_gap_count: usize,
    pub report_path: PathBuf,
}

/// Runs the VM supervision restart quality check.
///
/// Inputs:
/// - `root`: repository root containing the VM supervision runtime, exact
///   supervision tests, and Make target.
///
/// Output:
/// - Success summary and `vm-supervision-report.json` when the current restart
///   semantics are explicitly gated.
/// - Stable diagnostics when restart runtime anchors, exact tests, or Make
///   integration drift.
///
/// Transformation:
/// - Converts the current Slice 37 supervision baseline into an executable
///   report without overstating support for restart strategies that are still
///   open gaps.
pub fn run_vm_supervision_restart(root: &Path) -> QualityResult<VmSupervisionRestartSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_terms(
        root,
        "crates/terlan/src/runtime/vm/supervision.rs",
        REQUIRED_RUNTIME_ANCHORS,
        "VM supervision runtime",
    )?);
    diagnostics.extend(validate_terms(
        root,
        "crates/terlan/src/runtime/vm/supervision/shutdown.rs",
        REQUIRED_SHUTDOWN_ANCHORS,
        "VM supervision shutdown runtime",
    )?);
    diagnostics.extend(validate_terms(
        root,
        "crates/terlan/src/runtime/vm/supervision/runtime.rs",
        REQUIRED_PRODUCT_RUNTIME_ANCHORS,
        "product VM supervision runtime",
    )?);
    diagnostics.extend(validate_product_compilation(root)?);
    diagnostics.extend(validate_makefile(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-supervision-restart", &diagnostics));
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
        "schema": "terlan-vm-supervision-report-v1",
        "implementedPolicies": [
            "one_for_one",
            "one_for_all",
            "rest_for_one"
        ],
        "implementedRestartClasses": [
            "permanent",
            "transient",
            "temporary"
        ],
        "implementedRestartScheduling": [
            "exponential backoff",
            "per-child restart delay reporting",
            "observable last restart delay"
        ],
        "implementedShutdownSemantics": [
            "configured child shutdown timeout",
            "restart outcome shutdown timeout reporting",
            "observable last shutdown timeout",
            "cooperative shutdown message delivery",
            "VM timer-backed shutdown deadline enforcement",
            "clean shutdown deadline cancellation",
            "typed forced shutdown timeout exit"
        ],
        "implementedEscalationSemantics": [
            "restart intensity exhaustion marks supervisor failed",
            "failed supervisor records triggering child",
            "failed supervisor records terminal exit reason",
            "parent supervisor observes terminal child supervisor failure",
            "parent restart strategy rebuilds selected child-supervisor subtrees"
        ],
        "implementedRestartHistory": [
            "snapshot-visible restart history",
            "successful restart history entries",
            "terminal restart history entries",
            "non-restart history entries"
        ],
        "implementedProductIntegrations": [
            "NativeBoundary worker crash restart",
            "handler pool exhaustion restart",
            "in-flight request cancellation during restart"
        ],
        "supervisionFixtures": SUPERVISION_FIXTURES,
        "restartOutcomes": [
            "Restarted",
            "RestartedGroup",
            "NotRestarted",
            "LimitReached",
            "typed diagnostic"
        ],
        "observableState": [
            "supervisor id",
            "parent supervisor id",
            "supervisor name",
            "restart policy",
            "supervisor state",
            "supervisor failure child",
            "supervisor failure reason",
            "child supervisor failure state",
            "restart history",
            "restart history outcome",
            "child id",
            "child pid",
            "child source",
            "restart count",
            "restart limit",
            "restart class",
            "last restart delay",
            "shutdown timeout",
            "last shutdown timeout"
        ],
        "failureReasons": [
            "missing supervisor",
            "missing child",
            "duplicate child id",
            "missing process",
            "restart limit reached",
            "shutdown timeout"
        ],
        "supervisionGraphFields": [
            "supervisor id",
            "parent supervisor id",
            "supervisor name",
            "restart policy",
            "supervisor state",
            "child order",
            "child id",
            "child pid",
            "child source",
            "child restart class"
        ],
        "restartHistoryFields": [
            "child id",
            "old pid",
            "new pid",
            "restart count",
            "exit reason",
            "history outcome",
            "restart delay",
            "shutdown timeout"
        ],
        "escalationDecisionFields": [
            "triggering child",
            "child supervisor id",
            "terminal pid",
            "terminal reason",
            "restart count",
            "supervisor state"
        ],
        "finalProcessStateFields": [
            "old process exited",
            "new process runnable",
            "non-restarted process exited",
            "limit-reached process retained",
            "snapshot available after failed restart"
        ],
        "openRestartGaps": OPEN_RESTART_GAPS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM supervision report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{}: failed to write report: {err}", REPORT_PATH))?;

    Ok(VmSupervisionRestartSummary {
        fixture_count: SUPERVISION_FIXTURES.len(),
        exact_selector_count: REQUIRED_TEST_SELECTORS.len(),
        open_gap_count: OPEN_RESTART_GAPS.len(),
        report_path,
    })
}

fn validate_terms(
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
        .map_err(|err| format!("Makefile: failed to read VM supervision restart gate: {err}"))?;
    let mut diagnostics = Vec::new();
    if !text.contains("vm-supervision-restart-check: vm-supervision-primitives-check") {
        diagnostics.push(
            "Makefile: vm-supervision-restart-check must run after vm-supervision-primitives-check"
                .to_string(),
        );
    }
    if !text.contains("vm-supervision-restart") {
        diagnostics.push(
            "Makefile: VM supervision restart gate must run terlan-quality vm-supervision-restart"
                .to_string(),
        );
    }
    for selector in REQUIRED_TEST_SELECTORS {
        if !text.contains(selector) {
            diagnostics.push(format!(
                "Makefile: missing VM supervision exact selector `{selector}`"
            ));
        }
    }
    Ok(diagnostics)
}

fn validate_product_compilation(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("crates/terlan/src/runtime/vm.rs")).map_err(|err| {
        format!("crates/terlan/src/runtime/vm.rs: failed to read VM module graph: {err}")
    })?;
    let mut diagnostics = Vec::new();
    if !text.contains("pub mod supervision;") {
        diagnostics.push(
            "crates/terlan/src/runtime/vm.rs: supervision must be part of the shipping VM module graph"
                .to_string(),
        );
    }
    if text.contains("#[cfg(test)]\npub mod supervision;")
        || text.contains("#[cfg(test)]\npub(crate) mod supervision;")
    {
        diagnostics
            .push("crates/terlan/src/runtime/vm.rs: supervision cannot be test-only".to_string());
    }
    Ok(diagnostics)
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
#[path = "vm_supervision_restart_test.rs"]
#[cfg(test)]
mod vm_supervision_restart_test;
