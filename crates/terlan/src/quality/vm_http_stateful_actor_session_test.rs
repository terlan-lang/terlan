use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_http_stateful_actor_session, validate_entries_for_placeholder_terms,
    validate_no_placeholder_report_entries,
};

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
            "terlan-vm-http-stateful-actor-session-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/http_session.rs",
            r#"
VmHttpSessionRuntime VmHttpSessionRecoveryPolicy VmHttpSessionRoute VmHttpSessionAffinityKey
VmHttpSessionAffinityError lookup_or_create_with_affinity_keys resolve_http_session_affinity_key
crashed_session_actor_diagnostic session_actor_exit_reason
VmHttpSessionCommandOutcome VmHttpSessionPersistenceSnapshot VmHttpSessionMailboxBackpressure
VmHttpSessionWorkerMigration
VmHttpSessionHotReloadMigrationReport
apply_idempotent_command
state_version apply_state_update state_version_conflict_diagnostic
persistence_snapshot replay_persistence_snapshot
enqueue_actor_message actor_mailbox_backpressure
migrate_to_worker worker_migration_diagnostic
hot_reload_migration_compatibility_report hot_reload_migration_compatibility_diagnostic
VmHttpSessionLiveTemplateSubscriber subscribe_live_template unsubscribe_live_template
live_template_subscribers
VmHttpSessionSnapshot VmActorRuntime VmTableStore lookup_or_create
create_session lookup_existing rotate expire_due snapshots sticky_key
std.http.Session current set get delete with_response
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_session_test.rs",
            r#"
http_session_lookup_creates_actor_and_sticky_metadata
http_session_adapter_functions_delegate_to_actor_runtime
http_session_adapter_renders_non_string_values_for_string_get
http_session_blank_cookie_creates_replacement_session
http_session_affinity_accepts_single_typed_key
http_session_affinity_merges_duplicate_matching_keys
http_session_affinity_rejects_missing_and_conflicting_keys
http_session_table_event_adapters_are_defensive
http_session_delete_reports_stale_table_after_internal_cleanup
http_session_private_lookup_paths_report_stale_sessions
http_session_reuses_actor_and_table_state_for_cookie_lookup
http_session_actor_crash_during_request_cleans_state_and_replaces_cookie
http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state
http_session_idempotent_command_replays_duplicate_result_without_rerun
http_session_live_template_subscribers_are_cleaned_after_actor_exit
http_session_state_update_rejects_stale_concurrent_writer
http_session_persistence_snapshot_replays_after_restart
http_session_actor_mailbox_backpressure_is_attributed
http_session_migrates_durable_state_across_workers
http_session_reports_hot_reload_migration_compatibility
http_session_rotate_changes_cookie_without_losing_actor_state
http_session_expiration_cleans_actor_table_and_reports_stale
http_session_recovery_policy_can_fail_closed_for_stale_cookie
http_session_rejects_invalid_runtime_configuration
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-http-stateful-actor-session-check: vm-http-handler-scheduler-fairness-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_lookup_creates_actor_and_sticky_metadata -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_adapter_functions_delegate_to_actor_runtime -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_adapter_renders_non_string_values_for_string_get -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_blank_cookie_creates_replacement_session -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_affinity_accepts_single_typed_key -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_affinity_merges_duplicate_matching_keys -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_affinity_rejects_missing_and_conflicting_keys -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_table_event_adapters_are_defensive -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_delete_reports_stale_table_after_internal_cleanup -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_private_lookup_paths_report_stale_sessions -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_reuses_actor_and_table_state_for_cookie_lookup -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_actor_crash_during_request_cleans_state_and_replaces_cookie -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_idempotent_command_replays_duplicate_result_without_rerun -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_live_template_subscribers_are_cleaned_after_actor_exit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_state_update_rejects_stale_concurrent_writer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_persistence_snapshot_replays_after_restart -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_actor_mailbox_backpressure_is_attributed -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_migrates_durable_state_across_workers -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_reports_hot_reload_migration_compatibility -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_rotate_changes_cookie_without_losing_actor_state -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_expiration_cleans_actor_table_and_reports_stale -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_recovery_policy_can_fail_closed_for_stale_cookie -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_rejects_invalid_runtime_configuration -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_http_stateful_actor_session_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-stateful-actor-session
"#;

#[test]
fn vm_http_stateful_actor_session_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_stateful_actor_session(repo.root()).expect("quality check");

    assert_eq!(summary.affinity_fixture_count, 18);
    assert_eq!(summary.lifecycle_trace_count, 8);
    assert_eq!(summary.exact_selector_count, 26);
    assert_eq!(summary.rejected_session_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-stateful-actor-session-report-v1"));
    assert!(report.contains("affinityFixtures"));
    assert!(report.contains("actorLifecycleTraces"));
    assert!(report.contains("concurrentStateUpdates"));
    assert!(report.contains("persistenceHookReplay"));
    assert!(report.contains("backpressureCases"));
    assert!(report.contains("workerMigrationResults"));
    assert!(report.contains("hotReloadMigrationResults"));
    let report_json: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(report_json["reconnectCases"]["implemented"], true);
    assert_eq!(report_json["duplicateCommandHandling"]["implemented"], true);
    assert_eq!(
        report_json["liveTemplateSubscriberCleanup"]["implemented"],
        true
    );
    assert_eq!(report_json["concurrentStateUpdates"]["implemented"], true);
    assert_eq!(report_json["persistenceHookReplay"]["implemented"], true);
    assert_eq!(report_json["backpressureCases"]["implemented"], true);
    assert_eq!(report_json["workerMigrationResults"]["implemented"], true);
    assert_eq!(
        report_json["hotReloadMigrationResults"]["implemented"],
        true
    );
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_http_stateful_actor_session_rejects_missing_runtime_anchor() {
    let repo = TestRepo::new("missing-runtime-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let runtime = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/http_session.rs"),
    )
    .expect("read runtime");
    repo.write(
        "crates/terlan/src/runtime/vm/http_session.rs",
        &runtime.replace("VmHttpSessionRuntime", ""),
    )
    .expect("rewrite runtime");

    let error = run_vm_http_stateful_actor_session(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmHttpSessionRuntime"));
}

#[test]
fn vm_http_stateful_actor_session_rejects_missing_test_anchor() {
    let repo = TestRepo::new("missing-test-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let tests = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/http_session_test.rs"),
    )
    .expect("read tests");
    repo.write(
        "crates/terlan/src/runtime/vm/http_session_test.rs",
        &tests.replace(
            "http_session_rotate_changes_cookie_without_losing_actor_state",
            "",
        ),
    )
    .expect("rewrite tests");

    let error = run_vm_http_stateful_actor_session(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("http_session_rotate_changes_cookie_without_losing_actor_state"));
}

#[test]
fn vm_http_stateful_actor_session_rejects_missing_exact_selector() {
    let repo = TestRepo::new("missing-selector").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace(
            "runtime::vm::http_session::http_session_test::http_session_recovery_policy_can_fail_closed_for_stale_cookie",
            "runtime::vm::http_session::http_session_test::stale_cookie_is_not_checked",
        ),
    )
    .expect("rewrite makefile");

    let error = run_vm_http_stateful_actor_session(repo.root()).expect_err("selector should fail");

    assert!(error.contains("http_session_recovery_policy_can_fail_closed_for_stale_cookie"));
}

#[test]
fn vm_http_stateful_actor_session_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM HTTP stateful session report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("affinity fixtures", &["todo session behavior"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
