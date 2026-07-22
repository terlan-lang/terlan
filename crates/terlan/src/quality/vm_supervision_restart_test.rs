use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::run_vm_supervision_restart;

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-supervision-restart-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/src/runtime/vm/supervision.rs",
            r#"
VmSupervisionSystem VmSupervisorSnapshot VmSupervisorRestartHistoryEntry VmSupervisorRestartHistoryOutcome VmSupervisorState VmSupervisionRestart VmRestartPolicy
VmChildRestartClass VmRestartBackoffSchedule VmShutdownTimeout VmChildSpec create_supervisor create_child_supervisor_with_policy start_child restart_child snapshot
LimitReached Restarted RestartedGroup NotRestarted RestForOne Permanent Transient Temporary Failed ChildSupervisorFailed
last_restart_delay_ms restart_delay_ms shutdown_timeout_ms last_shutdown_timeout_ms restart_history parent_id
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/supervision/shutdown.rs",
            r#"
VmSupervisionShutdownQueue VmScheduledSupervisionShutdown VmSupervisionShutdownStart VmSupervisionShutdownCompletion
begin_shutdown complete_shutdown advance_clock handle_timer_event ShutdownTimeout
"#,
        )?;
        self.write(
            "Makefile",
            r#"
vm-supervision-restart-check: vm-supervision-primitives-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_relationship_test::actor_unlinked_child_termination_preserves_parent_mailbox_progress -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_starts_child_and_exposes_inspection_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_only_failed_child_for_one_for_one_policy -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_all_children_for_one_for_all_policy -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_one_for_all_enforces_restart_limit_before_group_restart -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_failed_and_later_children_for_rest_for_one_policy -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_rest_for_one_enforces_restart_limit_before_group_restart -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_temporary_child_never_restarts -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_transient_child_restarts_only_after_abnormal_exit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_skips_non_restartable_children_without_blocking_restartable_siblings -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_applies_exponential_restart_backoff_for_one_for_one_policy -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_reports_per_child_backoff_delays -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_shutdown_timeout_for_live_child_restart -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_reports_per_child_shutdown_timeouts -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_enforces_restart_limit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_supervisor_failure_when_restart_limit_escalates -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_propagates_child_supervisor_failure_to_parent_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_restart_history_for_restart_and_limit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_restart_history_for_non_restartable_child -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_child_diagnostic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_diagnostic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_rejects_duplicate_child_id -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restart_exits_live_child_before_restarting -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_process_instead_of_panicking_on_restart -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_for_restart_and_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_waits_for_clean_exit_and_cancels_deadline -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_deadline_forces_typed_exit_and_restarts_child -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_normal_exit_honors_transient_restart_class -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically -- --exact
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-supervision-restart
"#,
        )
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn vm_supervision_restart_writes_report_for_current_baseline() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_supervision_restart(repo.root()).expect("quality check");

    assert_eq!(summary.fixture_count, 30);
    assert_eq!(summary.exact_selector_count, 30);
    assert_eq!(summary.open_gap_count, 4);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-supervision-report-v1"));
    assert!(report.contains("one_for_one"));
    assert!(report.contains("one_for_all"));
    assert!(report.contains("rest_for_one"));
    assert!(report.contains("temporary"));
    assert!(report.contains("transient"));
    assert!(report.contains("NotRestarted"));
    assert!(report.contains("exponential backoff"));
    assert!(report.contains("last restart delay"));
    assert!(report.contains("configured child shutdown timeout"));
    assert!(report.contains("last shutdown timeout"));
    assert!(report.contains("VM timer-backed shutdown deadline enforcement"));
    assert!(report.contains("typed forced shutdown timeout exit"));
    assert!(report.contains("restart intensity exhaustion marks supervisor failed"));
    assert!(report.contains("parent supervisor observes terminal child supervisor failure"));
    assert!(report.contains("supervisor failure reason"));
    assert!(report.contains("parent supervisor id"));
    assert!(report.contains("child supervisor failure state"));
    assert!(report.contains("snapshot-visible restart history"));
    assert!(report.contains("restart history outcome"));
    assert!(report.contains("non-restart history entries"));
    assert!(report.contains("restart limit reached"));
    let json: Value = serde_json::from_str(&report).expect("json report");
    assert_json_array_contains(&json, "supervisionGraphFields", "child order");
    assert_json_array_contains(&json, "restartHistoryFields", "history outcome");
    assert_json_array_contains(&json, "escalationDecisionFields", "triggering child");
    assert_json_array_contains(&json, "escalationDecisionFields", "child supervisor id");
    assert_json_array_contains(
        &json,
        "finalProcessStateFields",
        "snapshot available after failed restart",
    );
    let gaps = json["openRestartGaps"].as_array().expect("open gaps");
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("one_for_all restart strategy")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("rest_for_one restart strategy")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("temporary/transient/permanent child classes")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("restart backoff schedule")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("shutdown timeout handling")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("shutdown timeout scheduler enforcement")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("supervisor death escalation")));
    assert!(!gaps
        .iter()
        .any(|gap| gap.as_str() == Some("parent supervisor failure propagation")));
    assert!(gaps
        .iter()
        .any(|gap| gap.as_str() == Some("parent supervisor restart strategy execution")));
}

fn assert_json_array_contains(json: &Value, key: &str, expected: &str) {
    let values = json[key].as_array().expect("json array");
    assert!(
        values.iter().any(|value| value.as_str() == Some(expected)),
        "{key} should contain {expected}"
    );
}

#[test]
fn vm_supervision_restart_rejects_missing_runtime_anchor() {
    let repo = TestRepo::new("missing-runtime-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let runtime = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/supervision.rs"),
    )
    .expect("read runtime");
    repo.write(
        "crates/terlan/src/runtime/vm/supervision.rs",
        &runtime.replace("VmSupervisionRestart", ""),
    )
    .expect("rewrite runtime");

    let error = run_vm_supervision_restart(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSupervisionRestart"));
}

#[test]
fn vm_supervision_restart_rejects_missing_make_selector() {
    let repo = TestRepo::new("missing-selector").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let makefile = fs::read_to_string(repo.root().join("Makefile")).expect("read makefile");
    repo.write(
        "Makefile",
        &makefile.replace(
            "runtime::vm::supervision::supervision_test::supervision_system_enforces_restart_limit",
            "runtime::vm::supervision::supervision_test::renamed_restart_limit",
        ),
    )
    .expect("rewrite makefile");

    let error = run_vm_supervision_restart(repo.root()).expect_err("selector should fail");

    assert!(error.contains("supervision_system_enforces_restart_limit"));
}
