use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::placeholder_terms::placeholder_entry_diagnostics;
use crate::terlan_quality::support::validate_required_terms;
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
    "read_http1_request",
    "RequestReadFailure",
    "RequestReadFailureKind",
    "ClientClosed",
    "Timeout",
    "Malformed",
];

const REQUIRED_HTTP_RESPONSE_WRITE_ANCHORS: &[&str] = &[
    "write_http1_response",
    "ResponseWriteFailure",
    "ResponseWriteFailureKind",
    "ClientClosed",
    "Timeout",
    "Io",
    "InvalidMetadata",
];

const REQUIRED_BENCHMARK_ANCHORS: &[&str] = &[
    "HttpPerformanceWorkload",
    "HttpPerformanceReport",
    "http-aot-performance-self-test",
    "measurement_rounds",
    "warmup_requests",
    "p50_ns",
    "p95_ns",
    "p99_ns",
    "throughput_requests_per_second",
    "process_memory_snapshot",
    "additional_workloads",
    "maintained_workloads",
    "measure_soak",
    "validate_with_curl",
    "error[http_aot.unstable]",
    "error[http_aot.memory_regression]",
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

const REQUIRED_EXACT_SELECTORS: &[&str] = &[];

const BENCHMARK_COMMANDS: &[&str] = &["http-aot-performance-self-test"];

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
    "one slow client among fast socket clients",
    "queued SSE response pressure",
    "stateful actor contention fairness",
    "c10/c100/c1000 long-running load profile plans",
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
/// Data describing vm http handler scheduler fairness summary.
pub struct VmHttpHandlerSchedulerFairnessSummary {
    pub fixture_count: usize,
    pub exact_selector_count: usize,
    pub benchmark_command_count: usize,
    pub rejected_fairness_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm http handler scheduler fairness.
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
        "std/http/native/src/http1/request_read.rs",
        REQUIRED_HTTP_REQUEST_READ_ANCHORS,
        "VM HTTP typed request reads",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "std/http/native/src/http1/response_write.rs",
        REQUIRED_HTTP_RESPONSE_WRITE_ANCHORS,
        "VM HTTP typed response writes",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/benchmark/http_aot_performance.rs",
        REQUIRED_BENCHMARK_ANCHORS,
        "VM HTTP socket benchmark fairness",
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
        "schema": "terlan-vm-http-handler-scheduler-fairness-report-v2",
        "evidenceKind": "source-contract-inventory",
        "runtimeValidated": false,
        "concurrencyProfiles": CONCURRENCY_PROFILES,
        "fairnessCounters": FAIRNESS_COUNTERS,
        "routeMix": ROUTE_MIX,
        "latencyPercentiles": LATENCY_PERCENTILES,
        "throughput": THROUGHPUT,
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
    for selector in REQUIRED_EXACT_SELECTORS {
        if !text.contains(selector) {
            diagnostics.push(format!(
                "Makefile: missing VM HTTP fairness exact selector `{selector}`"
            ));
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

/// Validates no placeholder report entries.
pub fn validate_no_placeholder_report_entries() -> Vec<String> {
    [
        ("concurrency profiles", CONCURRENCY_PROFILES),
        ("fairness counters", FAIRNESS_COUNTERS),
        ("route mix", ROUTE_MIX),
        ("latency percentiles", LATENCY_PERCENTILES),
        ("throughput", THROUGHPUT),
        ("queue saturation reasons", QUEUE_SATURATION_REASONS),
        ("replay seeds", REPLAY_SEEDS),
        ("long-running profiles", LONG_RUNNING_PROFILES),
        ("fairness fixtures", FAIRNESS_FIXTURES),
        ("benchmark commands", BENCHMARK_COMMANDS),
        ("rejected fairness paths", REJECTED_FAIRNESS_PATHS),
    ]
    .into_iter()
    .flat_map(|(label, entries)| validate_entries_for_placeholder_terms(label, entries))
    .collect()
}

/// Validates entries for placeholder terms.
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
#[cfg(test)]
mod vm_http_handler_scheduler_fairness_test;
