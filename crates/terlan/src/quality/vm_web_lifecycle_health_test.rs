use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_lifecycle_health, validate_no_placeholder_report_entries};

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
            "terlan-vm-web-lifecycle-health-{name}-{}-{unique}",
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
            "crates/terlan/src/commands/serve/mod.rs",
            r#"
startup_tx startup_rx failed to receive server startup status server startup
runtime_tls_config_for_serve
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/compose_check.rs",
            r#"
validate_postgres_healthcheck must define a healthcheck
healthcheck must not be disabled
healthcheck must define a non-empty test command
healthcheck_has_enabled_test healthcheck_command_is_enabled
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/compose_test.rs",
            r#"
validate_project_compose_rejects_postgres_without_healthcheck
validate_project_compose_rejects_disabled_postgres_healthcheck
validate_project_compose_rejects_postgres_healthcheck_without_test
validate_project_compose_rejects_postgres_healthcheck_none_test
validate_project_compose_accepts_postgres_dev_service
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/tls.rs",
            r#"
Maximum order-state refresh attempts while waiting for ACME readiness
challenge readiness Loads live TLS configuration for normal `terlc serve` startup
acme_runtime_tls_config_for_serve issue_acme_certificate load_acme_runtime_tls_cache
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http.rs",
            r#"
pub(crate) fn shutdown( pub(crate) fn shutdown_with_tls(
tcp.close_listener self.handlers.drain(..) finish_http1_tcp_handler
remove_listener_plan VmExitReason
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_test.rs",
            r#"
vm_http_tcp_server_cancels_parked_handler_and_closes_stream
vm_http_tcp_server_shutdown_closes_listener_and_active_handlers
vm_http_tcp_server_noop_poll_and_empty_shutdown_are_stable
vm_http_tcp_server_inspects_listener_pressure_and_handler_counters
vm_http_tcp_server_shutdown_with_tls_removes_listener_plan
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse_test.rs",
            r#"
vm_sse_stream_close_rejects_new_events_but_flushes_pending
stream.close() stream.inspect().closed flush queued
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
VmWebSocketCloseOutcome VmWebSocketTerminationReason VmWebSocketTermination
Timeout Cancelled
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket_test.rs",
            r#"
vm_websocket_session_closes_after_received_close_frame
vm_websocket_session_closes_after_sent_close_frame
vm_websocket_session_send_frame_closes_on_close_event
vm_websocket_runtime_remove_inactive_stream_sessions_prunes_closed_and_cancelled_streams
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/source_reload.rs",
            r#"
VmSourceReloadBatchReport changed_paths unique_source_paths ignored_paths
duplicate_source_paths publish_changed_files_with_report atomic batch boundary
event_snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/source_reload_test.rs",
            r#"
source_reload_adapter_publishes_changed_terlan_file_generations
source_reload_adapter_publishes_only_sources_from_mixed_batch
source_reload_adapter_rejects_invalid_mixed_batch_without_partial_publication
source_reload_adapter_reports_mixed_batch_diagnostics
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_force_local_writes_flushes_and_loads_snapshot
vm_distributed_storage_reports_flush_timeout_with_retry_recovery
requires_recovery retry_flush
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/instrumentation.rs",
            r#"
HotReload NodeDrain ServiceRestart VmOperatorPolicy audit_required
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
vm-web-lifecycle-health-check: vm-web-observability-check
	$(MAKE) web-compose-check
	$(MAKE) http-tls-check
	$(MAKE) vm-source-hot-reload-check
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_web_lifecycle_health_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-lifecycle-health
"#;

#[test]
fn vm_web_lifecycle_health_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_lifecycle_health(repo.root()).expect("quality check");

    assert_eq!(summary.lifecycle_state_transition_count, 5);
    assert_eq!(summary.health_endpoint_fixture_count, 4);
    assert_eq!(summary.drain_trace_count, 5);
    assert_eq!(summary.rejected_lifecycle_path_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-lifecycle-health-report-v1"));
    assert!(report.contains("startup: config validation"));
    assert!(report.contains("HTTP shutdown closes listener before draining handlers"));
    assert!(report.contains("force-kill support-bundle capture remains rejected"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_web_lifecycle_health_rejects_missing_healthcheck_anchor() {
    let repo = TestRepo::new("missing-healthcheck").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/compose_check.rs");
    let source = fs::read_to_string(&path).expect("compose source");
    repo.write(
        "crates/terlan/src/commands/serve/compose_check.rs",
        &source.replace("validate_postgres_healthcheck", ""),
    )
    .expect("rewrite compose source");

    let error = run_vm_web_lifecycle_health(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("validate_postgres_healthcheck"));
}

#[test]
fn vm_web_lifecycle_health_rejects_missing_stream_shutdown_anchor() {
    let repo = TestRepo::new("missing-stream-shutdown").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse_test.rs");
    let source = fs::read_to_string(&path).expect("sse test source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse_test.rs",
        &source.replace("stream.inspect().closed", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_web_lifecycle_health(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("stream.inspect().closed"));
}

#[test]
fn vm_web_lifecycle_health_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-source-hot-reload-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_lifecycle_health(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-source-hot-reload-check"));
}

#[test]
fn vm_web_lifecycle_health_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries(
        "lifecycle state transitions",
        &["ready-placeholder"],
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}
