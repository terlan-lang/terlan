use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_http_handler_scheduler_fairness, validate_entries_for_placeholder_terms,
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
            "terlan-vm-http-handler-scheduler-fairness-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/http.rs",
            r#"
VmHttpQueue VmHttpQueueMetrics enqueue_wait_count enqueue_wait_total_ns
dequeue_wait_count dequeue_wait_total_ns max_parked_producers
max_parked_consumers producer_wakeup_count consumer_wakeup_count
VmHttpFairnessReplaySeed build_http_fairness_replay_seed
poll_keep_alive_with_accept_limit poll_keep_alive_with_limits next_handler_index
skipped_blocked parked completed_total active_handlers inspect
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http/request_read.rs",
            r#"
read_http1_request_typed VmHttpRequestReadFailure VmHttpRequestReadFailureKind
client_closed request_timeout malformed_request
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http/response_wire.rs",
            r#"
write_http1_response_typed VmHttpResponseWriteFailure VmHttpResponseWriteFailureKind
client_closed_during_response_write response_write_timeout response_write_io_error
invalid_response_metadata
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/main.rs",
            r#"
benchmark_http_socket benchmark_http_socket_worker_count
benchmark_http_socket_acceptor_count spawn_benchmark_http_socket_acceptors
spawn_benchmark_http_socket_workers render_http_socket_benchmark_report p50_ns
p95_ns p99_ns throughput_requests_per_second queue_pressure request_mix
handler_delay_ms requests_per_connection large-static estimated_handler_reductions
response_write_wait slow-client slow_client_connections streaming text/event-stream
benchmark_sse_body_from_events synthetic-handlers
BenchmarkHttpLongRunningProfile benchmark_http_socket_long_running_profiles
target_concurrency sample_concurrency
runtime_attribution http_socket_benchmark_report_includes_runtime_phase_attribution
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/main/http_attribution.rs",
            r#"
terlan-vm-http-runtime-attribution-v1 accept_wait_ns request_read_parse_ns
route_match_ns request_decode_ns handler_run_ns synthetic_delay_ns
response_decode_encode_ns response_write_wait_ns dominantBottleneck
completedMatchesReductions connections_closed cancellations timeouts
request_read_cancellations request_read_timeouts response_write_cancellations
response_write_timeouts
schedulerPressure runnableProcessCount parkedProcessCount queueSaturationCount
backpressureWaitNs wakeupCount handlerRetryCount queueBalanced
parkedProcessesReleased saturationHasBackpressureOutcome
latencyBuckets transportNs parserNs schedulerNs routingNs allocationAndConversionNs
handlerNs responseWriteNs dominantCause sourceCounter phaseBucketsMatchAccountedTotal
handlerWorkloads static_handler_count json_handler_count add_handler_count
route_param_handler_count stateful_counter_handler_count
classifiedHandlerWorkloadsWithinCompleted
"#,
        )?;
        self.write(
            "crates/terlan/src/vm/main/http_attribution_test.rs",
            r#"
runtime_attribution_aggregates_phases_and_classifies_dominant_bottleneck
runtime_attribution_exposes_inconsistent_completion_accounting
runtime_attribution_preserves_typed_terminal_stage_reasons
runtime_attribution_reports_scheduler_pressure_and_consistency
runtime_attribution_rejects_unexplained_scheduler_saturation
runtime_attribution_buckets_every_measured_phase_once
runtime_attribution_classifies_scheduler_as_dominant_cause
runtime_attribution_classifies_deterministic_handler_workloads
"#,
        )?;
        self.write(
            "crates/terlan/src/benchmark/binary_protocol_http.rs",
            r#"
VmHttpReplayEvidence replay_determinism terlan-vm-http-replay-v1
execution_validated request_count validate_completion
parser_rejects_incomplete_execution_evidence
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
vm-http-handler-scheduler-fairness-check: vm-http-handler-dispatch-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_queue_rejects_zero_capacity -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_queue_preserves_fifo_order_and_metrics -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_queue_blocks_enqueue_until_consumer_frees_capacity -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_queue_blocks_dequeue_until_producer_adds_item -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_fairness_replay_seed_captures_queue_and_server_counters -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_fairness_replay_seed_rejects_empty_labels -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_accept_limit_bounds_accept_work_per_poll -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_limit_bounds_handler_work_per_poll -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_budget_uses_round_robin_cursor -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_parks_idle_handler_and_wakes_on_later_request -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_inspects_listener_pressure_and_handler_counters -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_cancel_adjusts_round_robin_cursor_edges -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::parse_http_socket_benchmark_accepts_queue_and_delay_options -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_uses_scheduler_sized_worker_pool -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_acceptor_pool_defaults_to_handler_width -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_rounds_warmup_to_whole_connections -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_report_includes_acceptor_and_handler_pool_counts -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_report_includes_per_handler_reduction_accounting -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_report_includes_response_write_wait_attribution -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_report_includes_runtime_phase_attribution -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_report_includes_slow_client_connection_count -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_crud_mix_covers_all_routes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_add_mix_uses_request_dependent_sum -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_synthetic_handler_mix_executes_all_handler_classes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_synthetic_counter_wire_validation_is_order_independent -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_synthetic_handler_replay_is_deterministic_across_fresh_vm_runs -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_replay_fingerprint_changes_when_the_workload_changes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_request_read_accounts_client_cancellation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_request_read_accounts_request_timeout -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_request_read_accepts_fragmented_slow_write -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_request_read_rejects_malformed_input_with_typed_reason -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_response_write_accepts_fragmented_slow_writer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_response_write_accounts_timeout -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_response_write_accounts_cancellation_storm -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_benchmark_response_write_rejects_other_io_with_typed_reason -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_large_static_mix_alternates_upload_and_static_routes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_slow_client_mix_marks_only_first_request_slow -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_streaming_mix_uses_sse_route -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_decodes_sse_response_descriptor -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_rejects_non_divisible_keep_alive_iterations -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm main_test::http_socket_benchmark_long_running_profiles_are_bounded_and_named -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_state_update_rejects_stale_concurrent_writer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_actor_mailbox_backpressure_is_attributed -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm http_attribution::http_attribution_test::runtime_attribution_classifies_deterministic_handler_workloads -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm http_attribution::http_attribution_test::runtime_attribution_preserves_typed_terminal_stage_reasons -- --exact
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 25 --concurrency 4 --queue-capacity 8
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 1 --handler-delay-ms 2
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 4 --queue-capacity 4 --requests-per-connection 4
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 24 --concurrency 4 --queue-capacity 4 --request-mix crud --payload-bytes 512
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 16 --concurrency 4 --queue-capacity 4 --request-mix large-static --payload-bytes 4096
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 8 --request-mix slow-client
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 24 --concurrency 6 --queue-capacity 4 --request-mix streaming
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 25 --concurrency 5 --queue-capacity 5 --request-mix synthetic-handlers
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-handler-scheduler-fairness
"#;

#[test]
fn vm_http_handler_scheduler_fairness_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_handler_scheduler_fairness(repo.root()).expect("quality check");

    assert_eq!(summary.fixture_count, 25);
    assert_eq!(summary.exact_selector_count, 45);
    assert_eq!(summary.benchmark_command_count, 8);
    assert_eq!(summary.rejected_fairness_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-handler-scheduler-fairness-report-v1"));
    assert!(report.contains("fairnessCounters"));
    assert!(report.contains("latencyPercentiles"));
    assert!(report.contains("support-bundle replay seeds for fairness regressions"));
    assert!(report.contains("large upload versus small static route mix fairness"));
    assert!(report.contains("per-handler reduction accounting in HTTP benchmark report"));
    assert!(report.contains("response-write wait attribution"));
    assert!(report.contains("one slow client among fast socket clients"));
    assert!(report.contains("queued SSE response pressure"));
    assert!(report.contains("stateful actor contention fairness"));
    assert!(report.contains("slow-client-c8"));
    assert!(report.contains("streaming-c6"));
    assert!(report.contains("stateful-actor-contention"));
    assert!(report.contains("c10/c100/c1000 long-running load profile plans"));
    assert!(report.contains("per-phase runtime attribution"));
    assert!(report.contains("terlan-vm-http-runtime-attribution-v1"));
    assert!(report.contains("completionConsistencyChecked"));
    assert!(report.contains("schedulerPressure"));
    assert!(report.contains("queueConsistencyChecked"));
    assert!(report.contains("saturationOutcomeChecked"));
    assert!(report.contains("latencyBuckets"));
    assert!(report.contains("dominantCauseCounterNamed"));
    assert!(report.contains("phaseBucketAccountingChecked"));
    assert!(report.contains("deterministic source-backed synthetic handler matrix"));
    assert!(report.contains("canonical replay fingerprints across fresh VM executions"));
    assert!(report.contains("terlan-vm-http-replay-v1"));
    assert!(report.contains("freshVmExecutionChecked"));
    assert!(
        report.contains("typed cancellation timeout and fragmented slow-write request outcomes")
    );
    assert!(report.contains("adversarialTerminalOutcomes"));
    assert!(report.contains("malformed_request"));
    assert!(
        report.contains("typed cancellation storm timeout and fragmented response-write outcomes")
    );
    assert!(report.contains("adversarialResponseWriteOutcomes"));
    assert!(report.contains("client_closed_during_response_write"));
    assert!(report.contains("response_write_timeout"));
    assert!(report.contains("long-running-c10"));
    assert!(report.contains("long-running-c100"));
    assert!(report.contains("long-running-c1000"));
}

#[test]
fn vm_http_handler_scheduler_fairness_rejects_missing_runtime_anchor() {
    let repo = TestRepo::new("missing-runtime-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let http = fs::read_to_string(repo.root().join("crates/terlan/src/runtime/vm/http.rs"))
        .expect("read http");
    repo.write(
        "crates/terlan/src/runtime/vm/http.rs",
        &http.replace("next_handler_index", ""),
    )
    .expect("rewrite http");

    let error =
        run_vm_http_handler_scheduler_fairness(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("next_handler_index"));
}

#[test]
fn vm_http_handler_scheduler_fairness_rejects_missing_benchmark_anchor() {
    let repo = TestRepo::new("missing-benchmark-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let main =
        fs::read_to_string(repo.root().join("crates/terlan/src/vm/main.rs")).expect("read main");
    repo.write(
        "crates/terlan/src/vm/main.rs",
        &main.replace("throughput_requests_per_second", ""),
    )
    .expect("rewrite main");

    let error =
        run_vm_http_handler_scheduler_fairness(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("throughput_requests_per_second"));
}

#[test]
fn vm_http_handler_scheduler_fairness_rejects_missing_benchmark_command() {
    let repo = TestRepo::new("missing-command").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace(
            "benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 1 --handler-delay-ms 2",
            "benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 8",
        ),
    )
    .expect("rewrite makefile");

    let error =
        run_vm_http_handler_scheduler_fairness(repo.root()).expect_err("command should fail");

    assert!(error.contains("--handler-delay-ms 2"));
}

#[test]
fn vm_http_handler_scheduler_fairness_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM HTTP scheduler fairness report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "concurrency profiles",
        &["tbd concurrency profile"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
