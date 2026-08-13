use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_vm_http_acme_worker;

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
            "terlan-vm-http-acme-worker-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/acme_worker.rs",
            r#"
VmAcmeWorkerRuntime VmAcmeWorkerRequest VmAcmeHttp01Challenge
VmAcmeWorkerState VmAcmeWorkerWake VmAcmeWorkerTelemetrySpan
VmAcmeWorkerAccessDecision VmAcmeWorkerExecutionLane
start_worker start_worker_for_lane validate_execution_lane prepare_http01_challenge
start_issuance begin_cache_write complete_worker schedule_renewal
shutdown_owner_workers inspect_worker capture_support_bundle_step
VmSupportBundleReplayResourceKind::AcmeWorker with_owner_limit
enforce_owner_backpressure telemetry_spans record_telemetry_span
challenge_route_access_decision park_issuance_waiter
VmAcmeWorkerWake::IssuanceReady
VmAcmeWorkerWake::RenewalDue renewal_due_wakeups
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/acme_runtime.rs",
            r#"
instant_acme AcmeHttp01Challenge acme_http01_challenge
runtime_tls_config_for_serve pending_http01_challenges rustls_server_config
store_acme_http01_challenge
start_live_acme_worker_for_serve VmAcmeWorkerRuntime
VmAcmeWorkerExecutionLane::Live VmProcessId::system_runtime_worker
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls.rs",
            r#"
VmTlsRuntime VmTlsMode::Auto build_listener_server_config
start_listener_server_connection remove_listener_plan rustls_server_config
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/request_dispatch.rs",
            r#"
acme_http01_challenge AcmeHttp01Challenge::Found AcmeHttp01Challenge::Missing
AcmeHttp01Challenge::Invalid
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/acme_worker_test.rs",
            r#"
vm_acme_worker_runs_http01_state_machine_without_network
vm_acme_worker_rejects_invalid_inputs_and_cleans_up_owner_workers
vm_acme_worker_captures_support_bundle_replay_steps
vm_acme_worker_enforces_owner_backpressure_limit
vm_acme_worker_emits_challenge_and_issuance_telemetry_spans
vm_acme_worker_authorizes_http01_challenge_route_through_policy_hook
vm_acme_worker_parks_and_wakes_issuance_waiters
vm_acme_worker_emits_due_renewal_wakeups
vm_acme_worker_uses_one_contract_for_fixture_and_live_lanes
vm_acme_worker_starts_issuance_without_new_challenge_for_valid_authorizations
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls/acme_runtime/tls_test.rs",
            r#"
serve_live_acme_issuance_starts_vm_worker_lane
pending_http01_challenges_reject_missing_http01
acme_http01_challenge_cache_writes_valid_token
acme_http01_challenge_cache_rejects_invalid_token
acme_certificate_cache_write_feeds_runtime_tls_config
runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata
runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata
runtime_tls_config_rejects_stale_auto_tls_certificate_cache
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test.rs",
            r#"
hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache
hyper_request_handler_serves_acme_http01_head_without_body
hyper_request_handler_returns_404_for_missing_acme_http01_challenge
hyper_request_handler_rejects_invalid_acme_http01_token
vm_stream_request_serves_acme_http01_challenge_without_hyper
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls_test.rs",
            r#"
vm_tls_runtime_rejects_auto_server_connection_without_cache
vm_tls_runtime_removes_tcp_listener_plan_on_shutdown
vm_tls_runtime_builds_manual_rustls_server_config
vm_tls_runtime_builds_internal_rustls_server_config
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
vm-http-acme-tls-base-check: vm-timer-deadline-check http-tls-check

vm-http-acme-worker-migration-check: vm-http-acme-tls-base-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-worker
"#;

#[test]
fn vm_http_acme_worker_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_acme_worker(repo.root()).expect("quality check");

    assert_eq!(summary.worker_state_trace_count, 14);
    assert_eq!(summary.challenge_routing_trace_count, 6);
    assert_eq!(summary.typed_diagnostic_fixture_count, 8);
    assert_eq!(summary.rejected_worker_path_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-acme-worker-report-v1"));
    assert!(report.contains("challenge route collision"));
    assert!(report.contains("instant_acme owns ACME protocol operations"));
    assert!(report.contains("VM-owned ACME worker state machine validated"));
    assert!(report.contains("support bundle captures VM worker state"));
    assert!(report.contains("owner-scoped issuance backpressure enforced"));
    assert!(report.contains("challenge and issuance telemetry spans emitted"));
    assert!(report.contains("VM access-policy hook validates challenge route"));
    assert!(report.contains("issuance waiters park and wake through VM scheduler handles"));
    assert!(report.contains("due renewal emits VM wakeup"));
    assert!(report.contains("deterministic and live lanes share one VM worker contract"));
    assert!(report.contains("serve auto TLS starts a VM-owned live ACME worker lane"));
    assert!(!report.contains("real VM-owned ACME worker runtime"));
    assert!(!report.contains("support-bundle capture from VM worker state"));
    assert!(!report.contains("VM backpressure hook for issuance queue limits"));
    assert!(!report.contains("VM telemetry hook for challenge and issuance spans"));
    assert!(!report.contains("VM HTTP access-policy hook for ACME challenge routing"));
    assert!(!report.contains("VM scheduler integration for issuance parking and wakeup"));
    assert!(!report.contains("renewal scheduler integration"));
    assert!(!report.contains("deterministic and live issuance sharing one worker contract"));
    assert!(!report.contains("named vm-http-acme-tls-production-check upstream gate"));
}

#[test]
fn vm_http_acme_worker_rejects_missing_serve_worker_handoff_anchor() {
    let repo = TestRepo::new("missing-serve-worker-handoff").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/tls/acme_runtime.rs");
    let source = fs::read_to_string(&path).expect("tls source");
    repo.write(
        "crates/terlan/src/commands/serve/tls/acme_runtime.rs",
        &source.replace("start_live_acme_worker_for_serve", ""),
    )
    .expect("rewrite tls source");

    let error = run_vm_http_acme_worker(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("start_live_acme_worker_for_serve"));
}

#[test]
fn vm_http_acme_worker_rejects_missing_challenge_routing_fixture() {
    let repo = TestRepo::new("missing-routing-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/serve_test.rs");
    let source = fs::read_to_string(&path).expect("serve test source");
    repo.write(
        "crates/terlan/src/commands/serve/serve_test.rs",
        &source.replace(
            "hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache",
            "",
        ),
    )
    .expect("rewrite serve test source");

    let error = run_vm_http_acme_worker(repo.root()).expect_err("fixture should fail");

    assert!(error.contains("hyper_request_handler_serves_acme_http01_challenge"));
}

#[test]
fn vm_http_acme_worker_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm-http-acme-worker", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_http_acme_worker(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-http-acme-worker"));
}
