use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_vm_http_acme_renewal;

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
            "terlan-vm-http-acme-renewal-{name}-{}-{unique}",
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
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
ACME_RENEWAL_INTERVAL ACME_METADATA_CLOCK_SKEW
validate_acme_certificate_cache_age validate_acme_certificate_cache_mode load_acme_runtime_tls_cache
issue_acme_certificate_cache_for_serve acme_runtime_tls_config_with_local_issuer
rustls_server_config
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/cache.rs",
            r#"
AcmeCertificateCacheMetadata renew_after_unix_seconds
store_acme_certificate_cache_metadata write_cache_file_atomically rename_cache_file
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
VmTimerTable start_one_shot cancel_owner_timers advance_clock VmTimerSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/acme_worker.rs",
            r#"
VmAcmeRenewalRetryPolicy delay_for_attempt
VmAcmeRenewalActor VmAcmeRenewalActorState
spawn_renewal_actor
begin_due_renewal
schedule_renewal_timer VmTimerTable start_one_shot
redact_acme_support_bundle_value acme.renewal.scheduled
capture_deterministic_renewal_cache_tls_handoff_replay
acme.renewal.replay.tls_handoff
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls.rs",
            r#"
VmTlsRuntime install_listener_plan remove_listener_plan
VmTlsRotationWindow rotate_listener_plan retire_rotation_window
start_listener_server_connection build_listener_server_config
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls_test.rs",
            r#"
runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata
runtime_tls_config_rejects_stale_auto_tls_certificate_cache
runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata
runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache
acme_runtime_tls_config_accepts_local_mock_issuer_cache_handoff
acme_runtime_tls_config_rejects_local_mock_issuer_without_cache_handoff
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer_test.rs",
            r#"
timer_table_starts_one_shot_timer_and_exposes_snapshot
timer_table_cancels_owner_timers_in_stable_order
timer_table_fires_due_timers_only_once
timer_table_receive_timeout_wakes_blocked_process
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/acme_worker_test.rs",
            r#"
vm_acme_renewal_retry_policy_is_typed_and_deterministic
vm_acme_renewal_actor_owns_worker_timer_and_shutdown_cleanup
vm_acme_worker_schedules_renewal_through_vm_timer_table
vm_acme_worker_denies_stale_challenge_access_after_renewal_scheduled
vm_acme_worker_records_renewal_telemetry_and_redacted_support_bundle_step
vm_acme_worker_routes_challenge_after_due_renewal_begins
vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls_test.rs",
            r#"
vm_tls_runtime_removes_tcp_listener_plan_on_shutdown
vm_tls_runtime_builds_manual_rustls_server_config
vm_tls_runtime_builds_internal_rustls_server_config
vm_tls_runtime_rejects_auto_server_connection_without_cache
vm_tls_runtime_enforces_rotation_overlap_window_before_retiring_old_config
vm_tls_runtime_hot_rotation_keeps_existing_connection_mode_for_old_accepts
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
vm-http-acme-renewal-rotation-check: vm-http-acme-cache-custody-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_renewal_retry_policy_is_typed_and_deterministic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_renewal_actor_owns_worker_timer_and_shutdown_cleanup -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::serve::tls::tls_test::runtime_tls_config_rejects_staging_mode_auto_tls_certificate_cache -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_schedules_renewal_through_vm_timer_table -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_denies_stale_challenge_access_after_renewal_scheduled -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_records_renewal_telemetry_and_redacted_support_bundle_step -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_routes_challenge_after_due_renewal_begins -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tls::tls_test::vm_tls_runtime_enforces_rotation_overlap_window_before_retiring_old_config -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tls::tls_test::vm_tls_runtime_hot_rotation_keeps_existing_connection_mode_for_old_accepts -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_http_acme_renewal_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-renewal
"#;

#[test]
fn vm_http_acme_renewal_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_acme_renewal(repo.root()).expect("quality check");

    assert_eq!(summary.renewal_schedule_count, 5);
    assert_eq!(summary.timer_trace_count, 5);
    assert_eq!(summary.tls_handoff_event_count, 5);
    assert_eq!(summary.rejected_renewal_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-acme-renewal-report-v1"));
    assert!(report.contains("host-runtime timer dependency rejected"));
    assert!(!report.contains("VM-owned ACME renewal actor"));
    assert!(!report.contains("staging/live endpoint mismatch rejection"));
    assert!(!report.contains("renewal timer wired to VmTimerTable"));
    assert!(!report.contains("access policy around renewal worker"));
    assert!(!report.contains("renewal telemetry and support-bundle redaction"));
    assert!(!report.contains("typed retry and jitter policy"));
    assert!(!report.contains("challenge routing during renewal"));
    assert!(!report.contains("old/new certificate overlap enforcement"));
    assert!(!report.contains("TLS hot rotation without dropping accepted connections"));
    assert!(report.contains("renewal metadata fixtures"));
    assert!(report.contains("renewal/cache/TLS handoff replay fixture"));
    assert!(!report.contains("deterministic replay of renewal/cache/TLS handoff"));
}

#[test]
fn vm_http_acme_renewal_rejects_missing_renewal_interval_anchor() {
    let repo = TestRepo::new("missing-renewal-interval").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/commands/serve/tls.rs");
    let source = fs::read_to_string(&path).expect("tls source");
    repo.write(
        "crates/terlan/src/commands/serve/tls.rs",
        &source.replace("ACME_RENEWAL_INTERVAL", ""),
    )
    .expect("rewrite tls source");

    let error = run_vm_http_acme_renewal(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("ACME_RENEWAL_INTERVAL"));
}

#[test]
fn vm_http_acme_renewal_rejects_missing_timer_fixture() {
    let repo = TestRepo::new("missing-timer-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/timer_test.rs");
    let source = fs::read_to_string(&path).expect("timer test source");
    repo.write(
        "crates/terlan/src/runtime/vm/timer_test.rs",
        &source.replace("timer_table_cancels_owner_timers_in_stable_order", ""),
    )
    .expect("rewrite timer test source");

    let error = run_vm_http_acme_renewal(repo.root()).expect_err("fixture should fail");

    assert!(error.contains("timer_table_cancels_owner_timers_in_stable_order"));
}

#[test]
fn vm_http_acme_renewal_rejects_missing_tls_shutdown_fixture() {
    let repo = TestRepo::new("missing-tls-shutdown").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/tls_test.rs");
    let source = fs::read_to_string(&path).expect("tls test source");
    repo.write(
        "crates/terlan/src/runtime/vm/tls_test.rs",
        &source.replace("vm_tls_runtime_removes_tcp_listener_plan_on_shutdown", ""),
    )
    .expect("rewrite tls test source");

    let error = run_vm_http_acme_renewal(repo.root()).expect_err("fixture should fail");

    assert!(error.contains("vm_tls_runtime_removes_tcp_listener_plan_on_shutdown"));
}

#[test]
fn vm_http_acme_renewal_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm_http_acme_renewal_test", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_http_acme_renewal(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm_http_acme_renewal_test"));
}
