use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_io_reactor_runtime, validate_entries_for_placeholder_terms,
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
            "terlan-vm-io-reactor-runtime-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/tcp.rs",
            r#"
VmTcpRuntime VmTcpWake connect_with_wakeups send_with_wakeups receive_with_wakeups
park_accept park_receive park_send close_write cancel_stream close_owner_streams
inspect_stream inspect_listener
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tcp_scheduler.rs",
            r#"
VmTcpWakeReport apply_tcp_wakeups wake_process accept_wakeups read_wakeups
write_wakeups diagnostics
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/io_reactor.rs",
            r#"
VmIoReactorLoop VmIoReactorWake VmIoReactorDrain enqueue_tcp_wake
enqueue_udp_wake enqueue_package_download_wake enqueue_debugger_wake
enqueue_acme_worker_wake enqueue_timer_event drain_ready deterministic_trace
wake_process
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/io_runtime_boundary.rs",
            r#"
VmExternalIoRuntimeBoundary VmExternalIoRuntimePlan
VmExternalIoSchedulingPolicy VmWakeProducerOnly OwnsActorScheduling
OwnsProcessContinuations DirectSchedulerAccess emits_typed_vm_wakeups
enforces_bounded_backpressure records_support_bundle_replay validate
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/udp.rs",
            r#"
VmUdpRuntime VmUdpSocket VmUdpWake bind_with_inbox_limit send_to_with_wakeups
park_receive receive_from cancel_owner_sockets inspect_socket
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/package_transport.rs",
            r#"
VmPackageDownloadRuntime VmPackageDownload VmPackageDownloadWake
VmPackageDownloadEvent start_download enqueue_chunk finish_download
park_receive receive_next cancel_owner_downloads inspect_download
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/support_bundle.rs",
            r#"
VmSupportBundleReplayMetadata VmSupportBundleReplayStep
VmSupportBundleReplayRecorder VmSupportBundleReplayResource
VmSupportBundleReplayExpectation record_io_step record_io_step_with_source
finish_bundle replay_steps_after verify_replay_step
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/io_diagnostics.rs",
            r#"
VmIoDiagnostic VmIoDiagnosticLog VmIoDiagnosticSourceMap
VmIoDiagnosticResource VmIoDiagnosticSeverity source_map_id
record_diagnostic diagnostics_for_source_map render_source_map_location
render_text
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/debugger_transport.rs",
            r#"
VmDebuggerTransportRuntime VmDebuggerSession VmDebuggerCommand
VmDebuggerEvent VmDebuggerWake open_session enqueue_command
park_command_receive receive_command enqueue_event park_event_receive
receive_event close_owner_sessions inspect_session
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
VmTimerTable start_receive_timeout advance_clock wake_process cancel_owner_timers
VmTimerEvent VmTimerSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/framing.rs",
            r#"
VmInMemoryFrameReader read_exact_with_timeout VmFramingError BackpressureExceeded
Timeout Cancelled FramingEof
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http.rs",
            r#"
VmHttpTcpServer poll_or_park_http1_tcp_exchange
poll_or_park_http1_tls_tcp_exchange_with_connection accept_http1_tcp_handler
finish_http1_tcp_handler cancel_handler shutdown tcp.park_receive actor.block
actor.wake
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/sse.rs",
            r#"
VmSseStream VmSseEndpointPlan BackpressureExceeded close inspect keep_alive_ms
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/websocket.rs",
            r#"
VmWebSocketRuntime VmWebSocketInboundQueue accept_upgrade terminate_session_and_stream
VmWebSocketTerminationReason Cancelled close_all_sessions
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/tls.rs",
            r#"
VmTlsRuntime VmTlsTcpServerStream VmTlsTcpPoll NeedRead Handshaking Ready
listener_transport_mode remove_listener_plan
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/acme_worker.rs",
            r#"
VmAcmeWorkerRuntime VmAcmeWorkerExecutionLane Live start_worker_for_lane
park_issuance_waiter start_issuance VmAcmeWorkerWake ChallengeReady
IssuanceReady RenewalDue schedule_renewal_timer spawn_renewal_actor
capture_support_bundle_step capture_deterministic_renewal_cache_tls_handoff_replay
"#,
        )?;
        self.write(
            "crates/terlan/src/quality/no_default_tokio_runtime.rs",
            r#"
no-default-tokio-runtime-check VM-owned runtime paths must not depend on Tokio
unexpected direct Tokio dependency
"#,
        )?;
        self.write(
            "Makefile",
            r#"
vm-io-reactor-runtime-check: vm-native-worker-runtime-check
	$(MAKE) no-default-tokio-runtime-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_accept_and_reports_wakeup_when_connection_arrives -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_receive_and_reports_wakeup_when_bytes_arrive -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_send_and_reports_wakeup_when_peer_drains_capacity -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::udp::udp_test::udp_runtime_delivers_packet_bursts_and_wakes_receiver -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::udp::udp_test::udp_runtime_enforces_backpressure_and_cancels_owner_sockets -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::package_transport::package_transport_test::package_download_transport_parks_and_wakes_when_chunk_arrives -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::package_transport::package_transport_test::package_download_transport_enforces_backpressure_and_cancels_owner -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::support_bundle::support_bundle_test::support_bundle_replay_metadata_records_ordered_io_steps -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::support_bundle::support_bundle_test::support_bundle_replay_metadata_rejects_mismatched_replay_identity -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_diagnostics::io_diagnostics_test::io_diagnostics_render_source_map_aware_runtime_failures -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_diagnostics::io_diagnostics_test::io_diagnostics_reject_malformed_source_map_context -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::debugger_transport::debugger_transport_test::debugger_transport_parks_and_wakes_command_and_event_receivers -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::debugger_transport::debugger_transport_test::debugger_transport_enforces_backpressure_and_closes_owner_sessions -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_accept_process -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_reactor::io_reactor_test::vm_io_reactor_loop_drains_mixed_wakeups_in_deterministic_order -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_reactor::io_reactor_test::vm_io_reactor_loop_reports_stale_processes_without_stopping_later_wakeups -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_accepts_vm_wakeup_only_byte_producer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_rejects_scheduling_and_hidden_continuations -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::io_runtime_boundary::io_runtime_boundary_test::external_io_runtime_boundary_rejects_unreplayable_or_unbounded_helpers -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_receive_timeout_wakes_blocked_process -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_timeout_for_pending_exact_read -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_cancelled_streams -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_backpressure_from_peer_inbox -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_cancels_parked_handler_and_closes_stream -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::sse::sse_test::vm_sse_stream_enforces_backpressure_and_event_size -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tls::tls_test::vm_tls_runtime_starts_manual_server_connection_with_readiness_state -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::tls::tls_test::vm_tls_tcp_server_stream_roundtrips_over_vm_tcp_runtime -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_uses_one_contract_for_fixture_and_live_lanes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_routes_challenge_after_due_renewal_begins -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::acme_worker::acme_worker_test::vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay -- --exact
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-io-reactor-runtime
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
fn vm_io_reactor_runtime_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_io_reactor_runtime(repo.root()).expect("quality check");

    assert_eq!(summary.fixture_count, 16);
    assert_eq!(summary.exact_selector_count, 32);
    assert_eq!(summary.rejected_runtime_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-io-reactor-runtime-report-v1"));
    assert!(report.contains("wakeupTraces"));
    assert!(report.contains("timerTraces"));
    assert!(report.contains("socketLifecycleTraces"));
    assert!(report.contains("UDP packet readiness"));
    assert!(report.contains("package download chunk readiness"));
    assert!(report.contains("support-bundle replay metadata"));
    assert!(report.contains("mismatched replay identities are rejected"));
    assert!(report.contains("source-map aware I/O diagnostics"));
    assert!(report.contains("diagnostics can be filtered by source_map_id"));
    assert!(report.contains("debugger transport readiness"));
    assert!(report.contains("owner cleanup closes sessions and makes handles stale"));
    assert!(report.contains("single unified I/O reactor loop"));
    assert!(report.contains("VmIoReactorLoop drains mixed readiness"));
    assert!(report.contains("stale reactor wakeups produce diagnostics"));
    assert!(report.contains("external async runtime scheduling boundary"));
    assert!(report.contains("external I/O helpers are validated as VM wake producers only"));
    assert!(report.contains("actor scheduling, process continuations"));
    assert!(report.contains("ACME live worker reactor integration"));
    assert!(report.contains("acmeLiveWorkerReactor"));
    assert!(report.contains("fixture and live lanes share one typed VM worker contract"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_tcp_anchor() {
    let repo = TestRepo::new("missing-tcp-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let tcp = fs::read_to_string(repo.root().join("crates/terlan/src/runtime/vm/tcp.rs"))
        .expect("read tcp");
    repo.write(
        "crates/terlan/src/runtime/vm/tcp.rs",
        &tcp.replace("receive_with_wakeups", ""),
    )
    .expect("rewrite tcp");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("receive_with_wakeups"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_stream_protocol_anchor() {
    let repo = TestRepo::new("missing-stream-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let websocket = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/websocket.rs"),
    )
    .expect("read websocket");
    repo.write(
        "crates/terlan/src/runtime/vm/websocket.rs",
        &websocket.replace("terminate_session_and_stream", ""),
    )
    .expect("rewrite websocket");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("terminate_session_and_stream"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_acme_worker_anchor() {
    let repo = TestRepo::new("missing-acme-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let acme = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/acme_worker.rs"),
    )
    .expect("read acme worker");
    repo.write(
        "crates/terlan/src/runtime/vm/acme_worker.rs",
        &acme.replace("capture_deterministic_renewal_cache_tls_handoff_replay", ""),
    )
    .expect("rewrite acme worker");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("capture_deterministic_renewal_cache_tls_handoff_replay"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_unified_reactor_anchor() {
    let repo = TestRepo::new("missing-reactor-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let reactor = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/io_reactor.rs"),
    )
    .expect("read reactor");
    repo.write(
        "crates/terlan/src/runtime/vm/io_reactor.rs",
        &reactor.replace("enqueue_package_download_wake", ""),
    )
    .expect("rewrite reactor");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("enqueue_package_download_wake"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_external_runtime_boundary_anchor() {
    let repo = TestRepo::new("missing-runtime-boundary-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let boundary = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/runtime/vm/io_runtime_boundary.rs"),
    )
    .expect("read boundary");
    repo.write(
        "crates/terlan/src/runtime/vm/io_runtime_boundary.rs",
        &boundary.replace("OwnsActorScheduling", ""),
    )
    .expect("rewrite boundary");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("OwnsActorScheduling"));
}

#[test]
fn vm_io_reactor_runtime_rejects_missing_gate_selector() {
    let repo = TestRepo::new("missing-selector").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let makefile = fs::read_to_string(repo.root().join("Makefile")).expect("read makefile");
    repo.write(
        "Makefile",
        &makefile.replace(
            "runtime::vm::timer::timer_test::timer_table_receive_timeout_wakes_blocked_process",
            "runtime::vm::timer::timer_test::renamed_timer_wakeup_test",
        ),
    )
    .expect("rewrite makefile");

    let error = run_vm_io_reactor_runtime(repo.root()).expect_err("selector should fail");

    assert!(error.contains("timer_table_receive_timeout_wakes_blocked_process"));
}

#[test]
fn vm_io_reactor_runtime_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM I/O reactor report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected =
        validate_entries_for_placeholder_terms("reactor fixtures", &["placeholder reactor case"]);
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
