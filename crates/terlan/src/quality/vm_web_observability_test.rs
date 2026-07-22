use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_web_observability, validate_no_placeholder_report_entries};

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
            "terlan-vm-web-observability-{name}-{}-{unique}",
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
            "crates/terlan/src/commands/serve/logging.rs",
            r#"
next_request_id connection_id_for_request request_id={request_id}
connection_id={connection_id} build_id={build_id}
method={request_method} path={request_path} route_method={}
handler={}.{} status={status} duration_ms={duration_ms}
source={}:{}:{} render_dev_error_page
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/serve_test.rs",
            r#"
render_handler_log_line_includes_handler_metadata
render_handler_log_line_includes_optional_source_metadata
render_static_log_line_includes_asset_metadata
render_static_route_log_line_includes_route_metadata
render_file_route_log_line_includes_route_and_file_metadata
render_dev_error_page_includes_escaped_handler_metadata
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http.rs",
            r#"
VmHttpQueueMetrics VmHttpTcpServerInfo VmHttpTcpServerPoll accepted_total
completed_total skipped_blocked enqueue_wait_count enqueue_wait_total_ns
inspect(&self, tcp: &VmTcpRuntime) handler_poll_limit
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_test.rs",
            r#"
vm_http_queue_preserves_fifo_order_and_metrics server.inspect(&tcp)
accepted_total completed_total
VM HTTP server handler poll limit must be greater than 0
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse.rs",
            r#"
VmSseStreamInfo pending_events max_pending_events emitted_events
inspect(&self) -> VmSseStreamInfo BackpressureExceeded
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
VmWebSocketInboundQueueInfo pending_frames queued_frame_bytes
inspect(&self) -> VmWebSocketInboundQueueInfo
error[vm_websocket_queue]: pending frame queue is full
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/code_server.rs",
            r#"
source_map_id Compiled module name and checksum/source-map metadata source_name
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/instrumentation.rs",
            r#"
VmRuntimeInspectionSnapshot VmProcessInspectionSnapshot VmDashboardRenderSnapshot
process_registry mailboxes reductions resource_handles native_call_state
"#,
        )?;
        self.write(
            "crates/terlan/src/benchmark/http_runtime_lane.rs",
            r#"
terlan-vm-http-runtime http_runtime RuntimeCapabilityStatus::Available
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
vm-web-observability-check: vm-web-config-secret-boundary-check
	$(MAKE) http-observability-check
	$(MAKE) vm-diagnostics-quality-check
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_web_observability_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-observability
"#;

#[test]
fn vm_web_observability_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_web_observability(repo.root()).expect("quality check");

    assert_eq!(summary.telemetry_field_count, 10);
    assert_eq!(summary.route_trace_count, 5);
    assert_eq!(summary.stream_trace_count, 5);
    assert_eq!(summary.rejected_observability_path_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-web-observability-report-v1"));
    assert!(report.contains("request_id"));
    assert!(report.contains("connection_id"));
    assert!(report.contains("template_stream_id.rejectedUntilLiveTemplateRuntimeEmission"));
    assert!(report.contains("SSE stream inspect exposes pending/emitted event counts"));
    assert!(report.contains("production trace sampling controls"));
    assert!(!report.contains("placeholder"));
    assert!(!report.contains("connection id emitted on every HTTP/WebSocket/SSE exchange"));
    assert!(report.contains("connection id emitted on every WebSocket/SSE exchange"));
}

#[test]
fn vm_web_observability_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries(
        "telemetry schema",
        &["template_stream_id-placeholder"],
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected placeholder report entry diagnostic: {diagnostics:?}"
    );
}

#[test]
fn vm_web_observability_rejects_missing_request_id_anchor() {
    let repo = TestRepo::new("missing-request-id").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/logging.rs");
    let source = fs::read_to_string(&path).expect("logging source");
    repo.write(
        "crates/terlan/src/commands/serve/logging.rs",
        &source.replace("request_id={request_id}", ""),
    )
    .expect("rewrite logging source");

    let error = run_vm_web_observability(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("request_id={request_id}"));
}

#[test]
fn vm_web_observability_rejects_missing_connection_id_anchor() {
    let repo = TestRepo::new("missing-connection-id").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/commands/serve/logging.rs");
    let source = fs::read_to_string(&path).expect("logging source");
    repo.write(
        "crates/terlan/src/commands/serve/logging.rs",
        &source.replace("connection_id={connection_id}", ""),
    )
    .expect("rewrite logging source");

    let error = run_vm_web_observability(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("connection_id={connection_id}"));
}

#[test]
fn vm_web_observability_rejects_missing_stream_inspection_anchor() {
    let repo = TestRepo::new("missing-stream-inspection").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/sse.rs");
    let source = fs::read_to_string(&path).expect("sse source");
    repo.write(
        "crates/terlan/src/runtime/vm/sse.rs",
        &source.replace("VmSseStreamInfo", ""),
    )
    .expect("rewrite sse source");

    let error = run_vm_web_observability(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmSseStreamInfo"));
}

#[test]
fn vm_web_observability_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate-term").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("$(MAKE) vm-diagnostics-quality-check", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_web_observability(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm-diagnostics-quality-check"));
}
