use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::placeholder_terms::placeholder_entry_diagnostics;
use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-http-handler-scheduler-fairness-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_HTTP_RUNTIME_ANCHORS: &[&str] = &[
    "VmHttpQueue",
    "VmHttpQueueMetrics",
    "VmHttpFairnessReplaySeed",
    "build_http_fairness_replay_seed",
    "enqueue_wait_count",
    "enqueue_wait_total_ns",
    "dequeue_wait_count",
    "dequeue_wait_total_ns",
    "max_parked_producers",
    "max_parked_consumers",
    "producer_wakeup_count",
    "consumer_wakeup_count",
    "poll_keep_alive_with_accept_limit",
    "poll_keep_alive_with_limits",
    "next_handler_index",
    "skipped_blocked",
    "parked",
    "completed_total",
    "active_handlers",
    "inspect",
];

const REQUIRED_HTTP_REQUEST_READ_ANCHORS: &[&str] = &[
    "read_http1_request_typed",
    "VmHttpRequestReadFailure",
    "VmHttpRequestReadFailureKind",
    "client_closed",
    "request_timeout",
    "malformed_request",
];

const REQUIRED_HTTP_RESPONSE_WRITE_ANCHORS: &[&str] = &[
    "write_http1_response_typed",
    "VmHttpResponseWriteFailure",
    "VmHttpResponseWriteFailureKind",
    "client_closed_during_response_write",
    "response_write_timeout",
    "response_write_io_error",
    "invalid_response_metadata",
];

const REQUIRED_BENCHMARK_ANCHORS: &[&str] = &[
    "benchmark_http_socket",
    "benchmark_http_socket_worker_count",
    "benchmark_http_socket_acceptor_count",
    "spawn_benchmark_http_socket_acceptors",
    "spawn_benchmark_http_socket_workers",
    "render_http_socket_benchmark_report",
    "p50_ns",
    "p95_ns",
    "p99_ns",
    "throughput_requests_per_second",
    "queue_pressure",
    "request_mix",
    "handler_delay_ms",
    "requests_per_connection",
    "large-static",
    "slow-client",
    "streaming",
    "synthetic-handlers",
    "text/event-stream",
    "benchmark_sse_body_from_events",
    "estimated_handler_reductions",
    "response_write_wait",
    "slow_client_connections",
    "BenchmarkHttpLongRunningProfile",
    "benchmark_http_socket_long_running_profiles",
    "target_concurrency",
    "sample_concurrency",
    "runtime_attribution",
];

const REQUIRED_RUNTIME_ATTRIBUTION_ANCHORS: &[&str] = &[
    "terlan-vm-http-runtime-attribution-v1",
    "accept_wait_ns",
    "request_read_parse_ns",
    "route_match_ns",
    "request_decode_ns",
    "handler_run_ns",
    "synthetic_delay_ns",
    "response_decode_encode_ns",
    "response_write_wait_ns",
    "dominantBottleneck",
    "latencyBuckets",
    "transportNs",
    "parserNs",
    "schedulerNs",
    "routingNs",
    "allocationAndConversionNs",
    "handlerNs",
    "responseWriteNs",
    "dominantCause",
    "sourceCounter",
    "phaseBucketsMatchAccountedTotal",
    "completedMatchesReductions",
    "schedulerPressure",
    "runnableProcessCount",
    "parkedProcessCount",
    "queueSaturationCount",
    "backpressureWaitNs",
    "wakeupCount",
    "handlerRetryCount",
    "queueBalanced",
    "parkedProcessesReleased",
    "saturationHasBackpressureOutcome",
    "connections_closed",
    "cancellations",
    "timeouts",
    "request_read_cancellations",
    "request_read_timeouts",
    "response_write_cancellations",
    "response_write_timeouts",
    "handlerWorkloads",
    "static_handler_count",
    "json_handler_count",
    "add_handler_count",
    "route_param_handler_count",
    "stateful_counter_handler_count",
    "classifiedHandlerWorkloadsWithinCompleted",
];

const REQUIRED_RUNTIME_ATTRIBUTION_TEST_ANCHORS: &[&str] = &[
    "runtime_attribution_aggregates_phases_and_classifies_dominant_bottleneck",
    "runtime_attribution_exposes_inconsistent_completion_accounting",
    "runtime_attribution_preserves_typed_terminal_stage_reasons",
    "runtime_attribution_reports_scheduler_pressure_and_consistency",
    "runtime_attribution_rejects_unexplained_scheduler_saturation",
    "runtime_attribution_buckets_every_measured_phase_once",
    "runtime_attribution_classifies_scheduler_as_dominant_cause",
    "runtime_attribution_classifies_deterministic_handler_workloads",
];

const REQUIRED_AOT_REPLAY_INTEGRATION_ANCHORS: &[&str] = &[
    "AotHandlerGeneration",
    "multicore_replay_evidence",
    "multicore_replay_capture",
    "VmMulticoreReplayEvidence",
];

const REQUIRED_AOT_REPLAY_EVIDENCE_ANCHORS: &[&str] = &[
    "terlan.vm.multicore-replay.v1",
    "VmMulticoreReplayEvidence",
    "retained_events",
    "dropped_events",
    "replayable",
];

const REQUIRED_EXACT_SELECTORS: &[&str] = &[
    "runtime::vm::http::http_test::vm_http_queue_rejects_zero_capacity",
    "runtime::vm::http::http_test::vm_http_queue_preserves_fifo_order_and_metrics",
    "runtime::vm::http::http_test::vm_http_queue_blocks_enqueue_until_consumer_frees_capacity",
    "runtime::vm::http::http_test::vm_http_queue_blocks_dequeue_until_producer_adds_item",
    "runtime::vm::http::http_test::vm_http_fairness_replay_seed_captures_queue_and_server_counters",
    "runtime::vm::http::http_test::vm_http_fairness_replay_seed_rejects_empty_labels",
    "runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_accept_limit_bounds_accept_work_per_poll",
    "runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_limit_bounds_handler_work_per_poll",
    "runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_budget_uses_round_robin_cursor",
    "runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_parks_idle_handler_and_wakes_on_later_request",
    "runtime::vm::http::http_test::vm_http_tcp_server_inspects_listener_pressure_and_handler_counters",
    "runtime::vm::http::http_test::vm_http_tcp_server_cancel_adjusts_round_robin_cursor_edges",
    "main_test::parse_http_socket_benchmark_accepts_queue_and_delay_options",
    "main_test::http_socket_benchmark_uses_scheduler_sized_worker_pool",
    "main_test::http_socket_benchmark_acceptor_pool_defaults_to_handler_width",
    "main_test::http_socket_benchmark_rounds_warmup_to_whole_connections",
    "main_test::http_socket_benchmark_report_includes_acceptor_and_handler_pool_counts",
    "main_test::http_socket_benchmark_report_includes_per_handler_reduction_accounting",
    "main_test::http_socket_benchmark_report_includes_response_write_wait_attribution",
    "main_test::http_socket_benchmark_report_includes_runtime_phase_attribution",
    "main_test::http_socket_benchmark_report_includes_slow_client_connection_count",
    "main_test::http_socket_benchmark_crud_mix_covers_all_routes",
    "main_test::http_socket_benchmark_add_mix_uses_request_dependent_sum",
    "main_test::http_socket_benchmark_synthetic_handler_mix_executes_all_handler_classes",
    "main_test::http_socket_benchmark_synthetic_counter_wire_validation_is_order_independent",
    "main_test::http_synthetic_handler_replay_is_deterministic_across_fresh_vm_runs",
    "main_test::http_replay_fingerprint_changes_when_the_workload_changes",
    "main_test::http_benchmark_request_read_accounts_client_cancellation",
    "main_test::http_benchmark_request_read_accounts_request_timeout",
    "main_test::http_benchmark_request_read_accepts_fragmented_slow_write",
    "main_test::http_benchmark_request_read_rejects_malformed_input_with_typed_reason",
    "main_test::http_benchmark_response_write_accepts_fragmented_slow_writer",
    "main_test::http_benchmark_response_write_accounts_timeout",
    "main_test::http_benchmark_response_write_accounts_cancellation_storm",
    "main_test::http_benchmark_response_write_rejects_other_io_with_typed_reason",
    "main_test::http_socket_benchmark_large_static_mix_alternates_upload_and_static_routes",
    "main_test::http_socket_benchmark_slow_client_mix_marks_only_first_request_slow",
    "main_test::http_socket_benchmark_streaming_mix_uses_sse_route",
    "main_test::http_socket_benchmark_decodes_sse_response_descriptor",
    "main_test::http_socket_benchmark_rejects_non_divisible_keep_alive_iterations",
    "main_test::http_socket_benchmark_long_running_profiles_are_bounded_and_named",
    "runtime::vm::http_session::http_session_test::http_session_state_update_rejects_stale_concurrent_writer",
    "runtime::vm::http_session::http_session_test::http_session_actor_mailbox_backpressure_is_attributed",
    "http_attribution::http_attribution_test::runtime_attribution_classifies_deterministic_handler_workloads",
    "http_attribution::http_attribution_test::runtime_attribution_preserves_typed_terminal_stage_reasons",
];

const BENCHMARK_COMMANDS: &[&str] = &[
    "benchmark-http-socket --iterations 25 --concurrency 4 --queue-capacity 8",
    "benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 1 --handler-delay-ms 2",
    "benchmark-http-socket --iterations 32 --concurrency 4 --queue-capacity 4 --requests-per-connection 4",
    "benchmark-http-socket --iterations 24 --concurrency 4 --queue-capacity 4 --request-mix crud --payload-bytes 512",
    "benchmark-http-socket --iterations 16 --concurrency 4 --queue-capacity 4 --request-mix large-static --payload-bytes 4096",
    "benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 8 --request-mix slow-client",
    "benchmark-http-socket --iterations 24 --concurrency 6 --queue-capacity 4 --request-mix streaming",
    "benchmark-http-socket --iterations 25 --concurrency 5 --queue-capacity 5 --request-mix synthetic-handlers",
];

const FAIRNESS_FIXTURES: &[&str] = &[
    "bounded HTTP scheduling queue",
    "enqueue backpressure metrics",
    "dequeue wakeup behavior",
    "accept-limit fairness",
    "handler-limit fairness",
    "round-robin handler cursor",
    "idle keep-alive wakeups",
    "listener/handler pressure inspection",
    "support-bundle replay seeds for fairness regressions",
    "socket benchmark pool sizing",
    "socket benchmark latency and throughput report",
    "large upload versus small static route mix fairness",
    "per-handler reduction accounting in HTTP benchmark report",
    "response-write wait attribution",
    "one slow client among fast socket clients",
    "queued SSE response pressure",
    "stateful actor contention fairness",
    "c10/c100/c1000 long-running load profile plans",
    "per-phase runtime attribution with dominant bottleneck classification",
    "scheduler pressure attribution with queue consistency invariants",
    "exclusive latency buckets with dominant runtime cause attribution",
    "deterministic source-backed synthetic handler matrix",
    "canonical replay fingerprints across fresh VM executions",
    "typed cancellation timeout and fragmented slow-write request outcomes",
    "typed cancellation storm timeout and fragmented response-write outcomes",
];

const CONCURRENCY_PROFILES: &[&str] = &[
    "queue-c1",
    "socket-c4",
    "socket-c8-pressure",
    "keep-alive-c4",
    "large-static-c4",
    "slow-client-c8",
    "streaming-c6",
    "stateful-actor-contention",
    "long-running-c10",
    "long-running-c100",
    "long-running-c1000",
];

const FAIRNESS_COUNTERS: &[&str] = &[
    "accepted",
    "polled",
    "parked",
    "skipped_blocked",
    "completed",
    "active_handlers",
    "enqueue_wait_count",
    "enqueue_wait_total_ns",
    "dequeue_wait_count",
    "dequeue_wait_total_ns",
    "max_parked_producers",
    "max_parked_consumers",
    "producer_wakeup_count",
    "consumer_wakeup_count",
];

const ROUTE_MIX: &[&str] = &[
    "single",
    "crud",
    "add",
    "large-static",
    "slow-client",
    "streaming",
    "synthetic-handlers",
];

const LATENCY_PERCENTILES: &[&str] = &["p50_ns", "p95_ns", "p99_ns"];

const THROUGHPUT: &[&str] = &["throughput_requests_per_second"];

const RUNTIME_ATTRIBUTION_PHASES: &[&str] = &[
    "accept_wait",
    "request_read_parse",
    "route_match",
    "request_decode",
    "handler_run",
    "synthetic_delay",
    "response_decode_encode",
    "response_write_wait",
];

const RUNTIME_TERMINAL_OUTCOMES: &[&str] = &[
    "completed_requests",
    "closed_connections",
    "cancellations",
    "timeouts",
];

const RUNTIME_SCHEDULER_PRESSURE: &[&str] = &[
    "runnable_process_count",
    "parked_process_count",
    "queue_depth",
    "queue_max_depth",
    "queue_saturation_count",
    "backpressure_wait_ns",
    "wakeup_count",
    "handler_retry_count",
];

const RUNTIME_LATENCY_BUCKETS: &[&str] = &[
    "transport",
    "parser",
    "scheduler",
    "routing",
    "allocation_conversion",
    "handler",
    "response_write",
];

const DOMINANT_BOTTLENECK: &str =
    "reported by queue pressure, handler delay, and listener/handler counters";

const QUEUE_SATURATION_REASONS: &[&str] = &[
    "bounded queue capacity",
    "accept limit",
    "handler poll limit",
];

const REPLAY_SEEDS: &[&str] = &[
    "socket-c4",
    "socket-c8-pressure",
    "keep-alive-c4",
    "slow-client-c8",
    "streaming-c6",
    "stateful-actor-contention",
    "long-running-c10",
    "long-running-c100",
    "long-running-c1000",
];

const LONG_RUNNING_PROFILES: &[&str] = &[
    "long-running-c10 target=10 sample=10",
    "long-running-c100 target=100 sample=16",
    "long-running-c1000 target=1000 sample=32",
];

const REJECTED_FAIRNESS_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmHttpHandlerSchedulerFairnessSummary {
    pub fixture_count: usize,
    pub exact_selector_count: usize,
    pub benchmark_command_count: usize,
    pub rejected_fairness_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_http_handler_scheduler_fairness(
    root: &Path,
) -> QualityResult<VmHttpHandlerSchedulerFairnessSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http.rs",
        REQUIRED_HTTP_RUNTIME_ANCHORS,
        "VM HTTP scheduler fairness",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http/request_read.rs",
        REQUIRED_HTTP_REQUEST_READ_ANCHORS,
        "VM HTTP typed request reads",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/http/response_wire.rs",
        REQUIRED_HTTP_RESPONSE_WRITE_ANCHORS,
        "VM HTTP typed response writes",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/vm/main.rs",
        REQUIRED_BENCHMARK_ANCHORS,
        "VM HTTP socket benchmark fairness",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/vm/main/http_attribution.rs",
        REQUIRED_RUNTIME_ATTRIBUTION_ANCHORS,
        "VM HTTP runtime attribution",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/vm/main/http_attribution_test.rs",
        REQUIRED_RUNTIME_ATTRIBUTION_TEST_ANCHORS,
        "VM HTTP runtime attribution adversarial tests",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/commands/serve/handler_cache/replay_evidence.rs",
        REQUIRED_AOT_REPLAY_INTEGRATION_ANCHORS,
        "VM HTTP AOT replay integration",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/multicore_replay.rs",
        REQUIRED_AOT_REPLAY_EVIDENCE_ANCHORS,
        "VM HTTP AOT replay evidence",
    )?);
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-http-handler-scheduler-fairness",
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
        "schema": "terlan-vm-http-handler-scheduler-fairness-report-v1",
        "concurrencyProfiles": CONCURRENCY_PROFILES,
        "fairnessCounters": FAIRNESS_COUNTERS,
        "routeMix": ROUTE_MIX,
        "latencyPercentiles": LATENCY_PERCENTILES,
        "throughput": THROUGHPUT,
        "runtimeAttribution": {
            "schema": "terlan-vm-http-runtime-attribution-v1",
            "phases": RUNTIME_ATTRIBUTION_PHASES,
            "terminalOutcomes": RUNTIME_TERMINAL_OUTCOMES,
            "schedulerPressure": RUNTIME_SCHEDULER_PRESSURE,
            "latencyBuckets": RUNTIME_LATENCY_BUCKETS,
            "dominantBottleneckClassified": true,
            "dominantCauseCounterNamed": true,
            "completionConsistencyChecked": true,
            "phaseBucketAccountingChecked": true,
            "queueConsistencyChecked": true,
            "saturationOutcomeChecked": true
        },
        "dominantBottleneck": DOMINANT_BOTTLENECK,
        "queueSaturationReasons": QUEUE_SATURATION_REASONS,
        "replaySeeds": REPLAY_SEEDS,
        "replayDeterminism": {
            "schema": "terlan.vm.multicore-replay.v1",
            "canonicalCounterOrder": true,
            "boundedSchedulerCaptureChecked": true,
            "droppedPrefixAccountingChecked": true
        },
        "adversarialTerminalOutcomes": {
            "clientCancellation": "client_closed",
            "requestTimeout": "request_timeout",
            "fragmentedSlowWriteAccepted": true,
            "malformedRequest": "malformed_request"
        },
        "adversarialResponseWriteOutcomes": {
            "clientCancellation": "client_closed_during_response_write",
            "responseTimeout": "response_write_timeout",
            "cancellationStormSize": 64,
            "fragmentedSlowWriteAccepted": true,
            "otherIoFailure": "response_write_io_error"
        },
        "longRunningProfiles": LONG_RUNNING_PROFILES,
        "fairnessFixtures": FAIRNESS_FIXTURES,
        "benchmarkCommands": BENCHMARK_COMMANDS,
        "rejectedFairnessPaths": REJECTED_FAIRNESS_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM HTTP fairness report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{}: failed to write report: {err}", REPORT_PATH))?;

    Ok(VmHttpHandlerSchedulerFairnessSummary {
        fixture_count: FAIRNESS_FIXTURES.len(),
        exact_selector_count: REQUIRED_EXACT_SELECTORS.len(),
        benchmark_command_count: BENCHMARK_COMMANDS.len(),
        rejected_fairness_count: REJECTED_FAIRNESS_PATHS.len(),
        report_path,
    })
}

fn validate_required_terms(
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
        .map_err(|err| format!("Makefile: failed to read VM HTTP fairness gate: {err}"))?;
    let mut diagnostics = Vec::new();
    if !text.contains("vm-http-handler-scheduler-fairness-check: vm-http-handler-dispatch-check") {
        diagnostics.push(
            "Makefile: VM HTTP fairness gate must run after vm-http-handler-dispatch-check"
                .to_string(),
        );
    }
    if !text.contains("vm-http-handler-scheduler-fairness") {
        diagnostics.push(
            "Makefile: VM HTTP fairness gate must run terlan-quality vm-http-handler-scheduler-fairness"
                .to_string(),
        );
    }
    let canonical_suite_owns_rust_tests = text.contains("COMPLETED_SLICE_RUST_GATES :=")
        && text.contains("vm-http-handler-scheduler-fairness-check \\")
        && text.contains("$(COMPLETED_SLICE_RUST_GATES): $(CANONICAL_RUST_SUITE_OWNER)");
    if !canonical_suite_owns_rust_tests {
        for selector in REQUIRED_EXACT_SELECTORS {
            if !text.contains(selector) {
                diagnostics.push(format!(
                    "Makefile: missing VM HTTP fairness exact selector `{selector}`"
                ));
            }
        }
    }
    for command in BENCHMARK_COMMANDS {
        if !text.contains(command) {
            diagnostics.push(format!(
                "Makefile: missing VM HTTP fairness benchmark command `{command}`"
            ));
        }
    }
    Ok(diagnostics)
}

pub fn validate_no_placeholder_report_entries() -> Vec<String> {
    [
        ("concurrency profiles", CONCURRENCY_PROFILES),
        ("fairness counters", FAIRNESS_COUNTERS),
        ("route mix", ROUTE_MIX),
        ("latency percentiles", LATENCY_PERCENTILES),
        ("throughput", THROUGHPUT),
        ("runtime attribution phases", RUNTIME_ATTRIBUTION_PHASES),
        ("runtime terminal outcomes", RUNTIME_TERMINAL_OUTCOMES),
        ("runtime scheduler pressure", RUNTIME_SCHEDULER_PRESSURE),
        ("runtime latency buckets", RUNTIME_LATENCY_BUCKETS),
        ("queue saturation reasons", QUEUE_SATURATION_REASONS),
        ("replay seeds", REPLAY_SEEDS),
        ("long-running profiles", LONG_RUNNING_PROFILES),
        ("fairness fixtures", FAIRNESS_FIXTURES),
        ("benchmark commands", BENCHMARK_COMMANDS),
        ("rejected fairness paths", REJECTED_FAIRNESS_PATHS),
    ]
    .into_iter()
    .flat_map(|(label, entries)| validate_entries_for_placeholder_terms(label, entries))
    .chain(validate_entries_for_placeholder_terms(
        "dominant bottleneck",
        &[DOMINANT_BOTTLENECK],
    ))
    .collect()
}

pub fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    placeholder_entry_diagnostics(
        label,
        entries,
        PLACEHOLDER_REPORT_TERMS,
        |label, entry, term| {
            format!("{label}: report entry `{entry}` contains placeholder term `{term}`")
        },
    )
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
#[path = "vm_http_handler_scheduler_fairness_test.rs"]
mod vm_http_handler_scheduler_fairness_test;
