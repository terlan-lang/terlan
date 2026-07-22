use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_performance, validate_entries_for_placeholder_terms,
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
            "terlan-vm-persistent-actor-performance-{name}-{}-{unique}",
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
            "crates/terlan/src/benchmark/persistent_actor.rs",
            "snapshot-append-replay correctness_verified throughput_events_per_second VmInMemoryPersistentActorStore VmFileBackedPersistentActorStore reopen_load vm_replay reopen_replay disk_bytes_p99 plan_persistent_actor_compaction events_retained VmPersistentActorMigrationGraph schema_migration execute_persistent_actor_adapter_cross_adapter_restore cross_adapter_restore measure_scheduler_attribution scheduler_overhead VmMemoryAccountant logical_value_bytes memory_high_water\n",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_performance.rs",
            r#"
VmPersistentActorPerformanceFixture
VmPersistentActorPerformanceBudget
VmPersistentActorPerformanceError
estimate_persistent_actor_performance_budget
CompactedEventsExceedTotal
throughput_events_per_tick
budget_pass
serialization_ticks
adapter_ticks
small_actor_fixture
event_storm_fixture
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/actor.rs",
            "VmMemoryAccountant send_value_message receive_message selective_receive_message memory_metrics synchronize_process\n",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/actor_test.rs",
            "actor_runtime_accounts_and_releases_mailbox_memory\n",
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_performance_test.rs",
            r#"
vm_persistent_actor_performance_estimates_small_actor_budget
vm_persistent_actor_performance_scales_event_storm_above_small_actor
vm_persistent_actor_performance_compaction_reduces_replay_budget
vm_persistent_actor_performance_rejects_empty_fixture_name_and_workload
vm_persistent_actor_performance_rejects_invalid_compaction_count
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
VmDistributedStorageSnapshot sequence checksum compact PartialWrite
ChecksumMismatch StaleSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state.rs",
            r#"
export_snapshot import_snapshot BTreeMap<VmDistributedStateScope
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
mailbox: VecDeque<VmMessage> mailbox_len selective_receive
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer.rs",
            r#"
VmTimerSnapshot deadline_tick: u64 pub(crate) fn snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
VmResourceSnapshot transfer_policy pub(crate) fn snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark
vm_distributed_storage_reports_finalize_and_partial_write_failures
vm_distributed_storage_rejects_stale_snapshot_replay
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_state_test.rs",
            r#"
vm_distributed_state_exports_and_imports_deterministic_snapshots
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process_test.rs",
            r#"
process_selective_receive_preserves_large_skipped_mailbox_prefix
process_exit_clears_mailbox_and_returns_resource_handles
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/timer_test.rs",
            r#"
timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)?;
        self.write(
            "target/quality/vm-persistent-actor-benchmark.json",
            COMPLETE_BENCHMARK_REPORT,
        )?;
        self.write(
            "benchmarks/baselines/vm-persistent-actor-runtime.json",
            COMPLETE_BASELINE,
        )
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-persistent-actor-performance-budget-check: vm-persistent-actor-adapter-conformance-check
	TERLAN_BENCH_PERSISTENT_ACTOR_OUTPUT=target/quality/vm-persistent-actor-benchmark.json $(CARGO) run --locked -p terlan --bin terlan-benchmark -- vm-persistent-actor-runtime-baseline
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_performance::persistent_actor_performance_test::vm_persistent_actor_performance_estimates_small_actor_budget -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_performance::persistent_actor_performance_test::vm_persistent_actor_performance_scales_event_storm_above_small_actor -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_performance::persistent_actor_performance_test::vm_persistent_actor_performance_compaction_reduces_replay_budget -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_performance::persistent_actor_performance_test::vm_persistent_actor_performance_rejects_empty_fixture_name_and_workload -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_performance::persistent_actor_performance_test::vm_persistent_actor_performance_rejects_invalid_compaction_count -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_performance_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-performance
"#;

const COMPLETE_BENCHMARK_REPORT: &str = r#"{
  "benchmark": "snapshot-append-replay",
  "adapter": "vm-in-memory",
  "run_count": 3,
  "samples_per_run": 100,
  "events_per_sample": 64,
  "schema": "terlan.vm-persistent-actor-benchmark.v1",
  "correctness_verified": true,
  "runs": [
    {"sample_count": 100, "p50_ns": 10, "p95_ns": 20, "p99_ns": 30, "throughput_events_per_second": 1000},
    {"sample_count": 100, "p50_ns": 11, "p95_ns": 21, "p99_ns": 31, "throughput_events_per_second": 990},
    {"sample_count": 100, "p50_ns": 12, "p95_ns": 22, "p99_ns": 32, "throughput_events_per_second": 980}
  ],
  "aggregate": {"sample_count": 300, "p50_ns": 11, "p95_ns": 21, "p99_ns": 31, "throughput_events_per_second": 990},
  "file_backed": {
    "run_count": 3,
    "samples_per_run": 20,
    "events_per_sample": 16,
    "correctness_verified": true,
    "runs": [
      {"sample_count": 20, "snapshot_commit": {"p50_ns": 10, "p95_ns": 20, "p99_ns": 30}, "append_events": {"p50_ns": 40, "p95_ns": 50, "p99_ns": 60}, "reopen_load": {"p50_ns": 15, "p95_ns": 20, "p99_ns": 25}, "vm_replay": {"p50_ns": 5, "p95_ns": 10, "p99_ns": 15}, "reopen_replay": {"p50_ns": 20, "p95_ns": 30, "p99_ns": 40}, "disk_bytes_p50": 100, "disk_bytes_p99": 100},
      {"sample_count": 20, "snapshot_commit": {"p50_ns": 10, "p95_ns": 20, "p99_ns": 30}, "append_events": {"p50_ns": 40, "p95_ns": 50, "p99_ns": 60}, "reopen_load": {"p50_ns": 15, "p95_ns": 20, "p99_ns": 25}, "vm_replay": {"p50_ns": 5, "p95_ns": 10, "p99_ns": 15}, "reopen_replay": {"p50_ns": 20, "p95_ns": 30, "p99_ns": 40}, "disk_bytes_p50": 100, "disk_bytes_p99": 100},
      {"sample_count": 20, "snapshot_commit": {"p50_ns": 10, "p95_ns": 20, "p99_ns": 30}, "append_events": {"p50_ns": 40, "p95_ns": 50, "p99_ns": 60}, "reopen_load": {"p50_ns": 15, "p95_ns": 20, "p99_ns": 25}, "vm_replay": {"p50_ns": 5, "p95_ns": 10, "p99_ns": 15}, "reopen_replay": {"p50_ns": 20, "p95_ns": 30, "p99_ns": 40}, "disk_bytes_p50": 100, "disk_bytes_p99": 100}
    ],
    "aggregate": {"sample_count": 60, "snapshot_commit": {"p50_ns": 10, "p95_ns": 20, "p99_ns": 30}, "append_events": {"p50_ns": 40, "p95_ns": 50, "p99_ns": 60}, "reopen_load": {"p50_ns": 15, "p95_ns": 20, "p99_ns": 25}, "vm_replay": {"p50_ns": 5, "p95_ns": 10, "p99_ns": 15}, "reopen_replay": {"p50_ns": 20, "p95_ns": 30, "p99_ns": 40}, "disk_bytes_p50": 100, "disk_bytes_p99": 100}
  },
  "compaction": {
    "run_count": 3,
    "samples_per_run": 100,
    "events_before": 1000,
    "events_retained": 200,
    "correctness_verified": true,
    "runs": [
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
    ],
    "aggregate": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
  },
  "schema_migration": {
    "run_count": 3,
    "samples_per_run": 100,
    "schema_versions": 64,
    "planned_edges": 63,
    "correctness_verified": true,
    "runs": [
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
    ],
    "aggregate": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
  },
  "cross_adapter_restore": {
    "run_count": 3,
    "samples_per_run": 100,
    "source_adapter": "embedded-key-value",
    "destination_adapter": "database-backed",
    "events_per_restore": 2,
    "correctness_verified": true,
    "runs": [
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200},
      {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
    ],
    "aggregate": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}
  },
  "scheduler_attribution": {
    "run_count": 3,
    "samples_per_run": 100,
    "events_per_sample": 64,
    "reductions_per_sample": 66,
    "correctness_verified": true,
    "runs": [
      {"sample_count": 100, "scheduler_ticks": 100, "reductions_charged": 6600, "scheduler_overhead": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}},
      {"sample_count": 100, "scheduler_ticks": 100, "reductions_charged": 6600, "scheduler_overhead": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}},
      {"sample_count": 100, "scheduler_ticks": 100, "reductions_charged": 6600, "scheduler_overhead": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}}
    ],
    "aggregate": {"sample_count": 300, "scheduler_ticks": 300, "reductions_charged": 19800, "scheduler_overhead": {"p50_ns": 100, "p95_ns": 150, "p99_ns": 200}}
  },
  "memory_high_water": {
    "run_count": 3,
    "samples_per_run": 100,
    "events_per_sample": 64,
    "soft_limit_bytes": 4194304,
    "hard_limit_bytes": 8388608,
    "correctness_verified": true,
    "budget_pass": true,
    "runs": [
      {"sample_count": 100, "p50_bytes": 1000, "p95_bytes": 1000, "p99_bytes": 1000},
      {"sample_count": 100, "p50_bytes": 1000, "p95_bytes": 1000, "p99_bytes": 1000},
      {"sample_count": 100, "p50_bytes": 1000, "p95_bytes": 1000, "p99_bytes": 1000}
    ],
    "aggregate": {"sample_count": 300, "p50_bytes": 1000, "p95_bytes": 1000, "p99_bytes": 1000}
  }
}"#;

const COMPLETE_BASELINE: &str = r#"{
  "schema": "terlan.vm-persistent-actor-budget.v1",
  "benchmark": "snapshot-append-replay",
  "adapter": "vm-in-memory",
  "required_run_count": 3,
  "minimum_samples_per_run": 100,
  "events_per_sample": 64,
  "maximum_p99_ns": 100,
  "minimum_throughput_events_per_second": 500
}"#;

#[test]
fn vm_persistent_actor_performance_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_performance(repo.root()).expect("quality check");

    assert_eq!(summary.fixture_budget_count, 8);
    assert_eq!(summary.deterministic_baseline_estimate_count, 5);
    assert_eq!(summary.timing_breakdown_count, 6);
    assert_eq!(summary.adversarial_performance_case_count, 8);
    assert_eq!(summary.rejected_budget_path_count, 0);
    assert_eq!(summary.measured_runtime_baseline_count, 1);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-performance-report-v1"));
    assert!(report.contains("\"p50\""));
    assert!(report.contains("small actor deterministic budget estimate"));
    assert!(report.contains("post-compaction replay budget reduction estimate"));
    assert!(report.contains("scheduler time"));
    assert!(report.contains("measuredRuntimeBaseline"));
    assert!(report.contains("correctness_verified"));
    assert!(report.contains("baselineComparison"));
    assert!(report.contains("file_backed"));
    assert!(report.contains("compaction"));
    assert!(report.contains("schema_migration"));
    assert!(report.contains("cross_adapter_restore"));
    assert!(report.contains("scheduler_attribution"));
    assert!(report.contains("memory_high_water"));
    assert!(!report.contains("real p50/p95/p99 persistent actor benchmark harness"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_memory_high_water_evidence() {
    let repo = TestRepo::new("invalid-memory-high-water").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace("\"soft_limit_bytes\": 4194304", "\"soft_limit_bytes\": 9000000")
        .replace("\"budget_pass\": true", "\"budget_pass\": false")
        .replace(
            "\"correctness_verified\": true,\n    \"budget_pass\": false,\n    \"runs\": [\n      {\"sample_count\": 100, \"p50_bytes\": 1000, \"p95_bytes\": 1000, \"p99_bytes\": 1000",
            "\"correctness_verified\": false,\n    \"budget_pass\": false,\n    \"runs\": [\n      {\"sample_count\": 100, \"p50_bytes\": 0, \"p95_bytes\": 1000, \"p99_bytes\": 9000000",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid memory high-water benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid memory high-water evidence should fail");
    assert!(error.contains("must verify account and release metrics"));
    assert!(error.contains("must pass its hard budget"));
    assert!(error.contains("limits must be positive and ordered"));
    assert!(error.contains("byte percentiles must be positive and monotonic"));
    assert!(error.contains("p99 must not exceed the hard memory budget"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_scheduler_attribution() {
    let repo = TestRepo::new("invalid-scheduler-attribution").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace("\"reductions_per_sample\": 66", "\"reductions_per_sample\": 65")
        .replace(
            "\"correctness_verified\": true,\n    \"runs\": [\n      {\"sample_count\": 100, \"scheduler_ticks\": 100, \"reductions_charged\": 6600, \"scheduler_overhead\": {\"p50_ns\": 100",
            "\"correctness_verified\": false,\n    \"runs\": [\n      {\"sample_count\": 100, \"scheduler_ticks\": 99, \"reductions_charged\": 6600, \"scheduler_overhead\": {\"p50_ns\": 0",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid scheduler attribution benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid scheduler attribution should fail");
    assert!(error.contains("must verify VM tick and reduction accounting"));
    assert!(error.contains("reductions/sample must cover snapshot, events, and replay"));
    assert!(error.contains("must attribute one scheduler tick per sample"));
    assert!(error.contains("reductions must match the scheduled workload"));
    assert!(error.contains("scheduler overhead percentiles must be positive"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_cross_adapter_restore_evidence() {
    let repo = TestRepo::new("invalid-cross-adapter-restore").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace(
            "\"destination_adapter\": \"database-backed\"",
            "\"destination_adapter\": \"embedded-key-value\"",
        )
        .replace("\"events_per_restore\": 2", "\"events_per_restore\": 0")
        .replace(
            "\"events_per_restore\": 0,\n    \"correctness_verified\": true,\n    \"runs\": [\n      {\"p50_ns\": 100",
            "\"events_per_restore\": 0,\n    \"correctness_verified\": false,\n    \"runs\": [\n      {\"p50_ns\": 0",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid cross-adapter restore benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid cross-adapter restore evidence should fail");
    assert!(error.contains("must verify destination replay correctness"));
    assert!(error.contains("requires distinct non-empty source and destination adapters"));
    assert!(error.contains("must replay at least one event"));
    assert!(error.contains("cross-adapter restore run 0 percentiles must be positive"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_schema_migration_evidence() {
    let repo = TestRepo::new("invalid-schema-migration").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace("\"planned_edges\": 63", "\"planned_edges\": 64")
        .replace(
            "\"schema_versions\": 64,\n    \"planned_edges\": 64,\n    \"correctness_verified\": true,\n    \"runs\": [\n      {\"p50_ns\": 100",
            "\"schema_versions\": 64,\n    \"planned_edges\": 64,\n    \"correctness_verified\": false,\n    \"runs\": [\n      {\"p50_ns\": 0",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid schema migration benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid schema migration evidence should fail");
    assert!(error.contains("schema migration benchmark must verify ordered chain correctness"));
    assert!(error.contains("schema migration must plan one ordered edge"));
    assert!(error.contains("schema migration run 0 percentiles must be positive"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_compaction_evidence() {
    let repo = TestRepo::new("invalid-compaction").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace("\"events_retained\": 200", "\"events_retained\": 1000")
        .replace(
            "\"correctness_verified\": true,\n    \"runs\": [\n      {\"p50_ns\": 100",
            "\"correctness_verified\": false,\n    \"runs\": [\n      {\"p50_ns\": 0",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid compaction benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid compaction evidence should fail");
    assert!(error.contains("compaction benchmark must verify retained suffix correctness"));
    assert!(error.contains("compaction must retain a non-empty proper event suffix"));
    assert!(error.contains("compaction run 0 percentiles must be positive"));
}

#[test]
fn vm_persistent_actor_performance_rejects_invalid_file_backed_evidence() {
    let repo = TestRepo::new("invalid-file-backed").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let invalid = COMPLETE_BENCHMARK_REPORT
        .replace(
            "\"correctness_verified\": true,\n    \"runs\"",
            "\"correctness_verified\": false,\n    \"runs\"",
        )
        .replace(
            "\"disk_bytes_p99\": 100}\n  }",
            "\"disk_bytes_p99\": 0}\n  }",
        )
        .replace(
            "\"vm_replay\": {\"p50_ns\": 5",
            "\"vm_replay\": {\"p50_ns\": 0",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &invalid,
    )
    .expect("write invalid file-backed benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("invalid file-backed evidence should fail");
    assert!(error.contains("file-backed benchmark must verify durable replay"));
    assert!(error.contains("file-backed aggregate disk bytes must be positive"));
    assert!(error.contains("`vm_replay` percentiles must be positive"));
}

#[test]
fn vm_persistent_actor_performance_rejects_latency_and_throughput_regressions() {
    let repo = TestRepo::new("runtime-regression").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let regressed = COMPLETE_BENCHMARK_REPORT
        .replace("\"p99_ns\": 31", "\"p99_ns\": 101")
        .replace(
            "\"throughput_events_per_second\": 990}",
            "\"throughput_events_per_second\": 499}",
        );
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &regressed,
    )
    .expect("write regressed benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("regressed benchmark should fail");
    assert!(error.contains("p99 101 ns exceeds baseline 100 ns"));
    assert!(error.contains("throughput 499 events/sec is below baseline 500"));
}

#[test]
fn vm_persistent_actor_performance_rejects_unverified_measured_runtime() {
    let repo = TestRepo::new("unverified-runtime").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "target/quality/vm-persistent-actor-benchmark.json",
        &COMPLETE_BENCHMARK_REPORT.replace(
            "\"correctness_verified\": true",
            "\"correctness_verified\": false",
        ),
    )
    .expect("write invalid benchmark");

    let error = run_vm_persistent_actor_performance(repo.root())
        .expect_err("unverified benchmark should fail");
    assert!(error.contains("benchmark must verify replay correctness"));
}

#[test]
fn vm_persistent_actor_performance_rejects_missing_storage_anchor() {
    let repo = TestRepo::new("missing-storage").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/distributed_storage.rs");
    let source = fs::read_to_string(&path).expect("storage source");
    repo.write(
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &source.replace("PartialWrite", ""),
    )
    .expect("rewrite storage source");

    let error = run_vm_persistent_actor_performance(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("PartialWrite"));
}

#[test]
fn vm_persistent_actor_performance_rejects_missing_fixture_anchor() {
    let repo = TestRepo::new("missing-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/process_test.rs");
    let source = fs::read_to_string(&path).expect("process test source");
    repo.write(
        "crates/terlan/src/runtime/vm/process_test.rs",
        &source.replace(
            "process_selective_receive_preserves_large_skipped_mailbox_prefix",
            "",
        ),
    )
    .expect("rewrite process test source");

    let error = run_vm_persistent_actor_performance(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("large_skipped_mailbox_prefix"));
}

#[test]
fn vm_persistent_actor_performance_rejects_missing_adapter_timing_anchor() {
    let repo = TestRepo::new("missing-adapter-timing").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_performance.rs");
    let source = fs::read_to_string(&path).expect("persistent actor performance source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_performance.rs",
        &source.replace("adapter_ticks", ""),
    )
    .expect("rewrite persistent actor performance source");

    let error = run_vm_persistent_actor_performance(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("adapter_ticks"));
}

#[test]
fn vm_persistent_actor_performance_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm_persistent_actor_performance_test", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_performance(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm_persistent_actor_performance_test"));
}

#[test]
fn vm_persistent_actor_performance_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor performance report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "adversarial performance cases",
        &["todo event storm benchmark"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
