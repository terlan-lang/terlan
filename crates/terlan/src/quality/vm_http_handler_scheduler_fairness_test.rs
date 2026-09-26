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
ClientClosed Timeout Malformed
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http/response_wire.rs",
            r#"
write_http1_response_typed VmHttpResponseWriteFailure VmHttpResponseWriteFailureKind
ClientClosed Timeout Io InvalidMetadata
"#,
        )?;
        self.write(
            "crates/terlan/src/benchmark/http_aot_performance.rs",
            r#"
HttpPerformanceWorkload HttpPerformanceReport http-aot-performance-self-test
measurement_rounds warmup_requests p50_ns p95_ns p99_ns
throughput_requests_per_second process_memory_snapshot additional_workloads
maintained_workloads measure_soak validate_with_curl
error[http_aot.unstable] error[http_aot.memory_regression]
"#,
        )?;
        self.write(
            "crates/terlan/src/commands/serve/handler_cache/replay_evidence.rs",
            r#"
AotHandlerGeneration multicore_replay_evidence multicore_replay_capture
VmMulticoreReplayEvidence
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/multicore_replay.rs",
            r#"
terlan.vm.multicore-replay.v1 VmMulticoreReplayEvidence
retained_events dropped_events replayable
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
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --features benchmark-tools --quiet -- http-aot-performance-self-test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-handler-scheduler-fairness
"#;

#[test]
fn vm_http_handler_scheduler_fairness_writes_report_for_complete_gate() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_http_handler_scheduler_fairness(repo.root()).expect("quality check");

    assert_eq!(summary.fixture_count, 19);
    assert_eq!(summary.exact_selector_count, 0);
    assert_eq!(summary.benchmark_command_count, 1);
    assert_eq!(summary.rejected_fairness_count, 0);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-http-handler-scheduler-fairness-report-v2"));
    let value: serde_json::Value = serde_json::from_str(&report).expect("report JSON");
    assert_eq!(value["evidenceKind"], "source-contract-inventory");
    assert_eq!(value["runtimeValidated"], false);
    assert!(value.get("runtimeAttribution").is_none());
    assert!(value.get("dominantBottleneck").is_none());
    assert!(report.contains("fairnessCounters"));
    assert!(report.contains("latencyPercentiles"));
    assert!(report.contains("support-bundle replay seeds for fairness regressions"));
    assert!(report.contains("large upload versus small static route mix fairness"));
    assert!(report.contains("one slow client among fast socket clients"));
    assert!(report.contains("queued SSE response pressure"));
    assert!(report.contains("stateful actor contention fairness"));
    assert!(report.contains("slow-client-c8"));
    assert!(report.contains("streaming-c6"));
    assert!(report.contains("stateful-actor-contention"));
    assert!(report.contains("c10/c100/c1000 long-running load profile plans"));
    assert!(report.contains("canonical replay fingerprints across fresh VM executions"));
    assert!(report.contains("terlan.vm.multicore-replay.v1"));
    assert!(report.contains("boundedSchedulerCaptureChecked"));
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
    let main = fs::read_to_string(
        repo.root()
            .join("crates/terlan/src/benchmark/http_aot_performance.rs"),
    )
    .expect("read main");
    repo.write(
        "crates/terlan/src/benchmark/http_aot_performance.rs",
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
        &COMPLETE_MAKEFILE.replace("http-aot-performance-self-test", "removed-aot-self-test"),
    )
    .expect("rewrite makefile");

    let error =
        run_vm_http_handler_scheduler_fairness(repo.root()).expect_err("command should fail");

    assert!(error.contains("http-aot-performance-self-test"));
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
