use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-performance-report.json";
const BENCHMARK_REPORT_PATH: &str = "target/quality/vm-persistent-actor-benchmark.json";
const BASELINE_PATH: &str = "benchmarks/baselines/vm-persistent-actor-runtime.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/persistent_actor_performance.rs",
        &[
            "VmPersistentActorPerformanceFixture",
            "VmPersistentActorPerformanceBudget",
            "VmPersistentActorPerformanceError",
            "estimate_persistent_actor_performance_budget",
            "CompactedEventsExceedTotal",
            "throughput_events_per_tick",
            "budget_pass",
            "serialization_ticks",
            "adapter_ticks",
            "small_actor_fixture",
            "event_storm_fixture",
        ],
    ),
    (
        "crates/terlan/src/benchmark/persistent_actor.rs",
        &[
            "snapshot-append-replay",
            "correctness_verified",
            "throughput_events_per_second",
            "VmInMemoryPersistentActorStore",
            "VmFileBackedPersistentActorStore",
            "reopen_load",
            "vm_replay",
            "reopen_replay",
            "disk_bytes_p99",
            "plan_persistent_actor_compaction",
            "events_retained",
            "VmPersistentActorMigrationGraph",
            "schema_migration",
            "execute_persistent_actor_adapter_cross_adapter_restore",
            "cross_adapter_restore",
            "measure_scheduler_attribution",
            "scheduler_overhead",
            "VmMemoryAccountant",
            "logical_value_bytes",
            "memory_high_water",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/actor.rs",
        &[
            "VmMemoryAccountant",
            "send_value_message",
            "receive_message",
            "selective_receive_message",
            "memory_metrics",
            "synchronize_process",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &[
            "VmDistributedStorageSnapshot",
            "sequence",
            "checksum",
            "compact",
            "PartialWrite",
            "ChecksumMismatch",
            "StaleSnapshot",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        &[
            "export_snapshot",
            "import_snapshot",
            "BTreeMap<VmDistributedStateScope",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/process.rs",
        &[
            "mailbox: VecDeque<VmMessage>",
            "mailbox_len",
            "selective_receive",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer.rs",
        &[
            "VmTimerSnapshot",
            "deadline_tick: u64",
            "pub(crate) fn snapshots",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource.rs",
        &[
            "VmResourceSnapshot",
            "transfer_policy",
            "pub(crate) fn snapshots",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/actor_test.rs",
        &["actor_runtime_accounts_and_releases_mailbox_memory"],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_performance_test.rs",
        &[
            "vm_persistent_actor_performance_estimates_small_actor_budget",
            "vm_persistent_actor_performance_scales_event_storm_above_small_actor",
            "vm_persistent_actor_performance_compaction_reduces_replay_budget",
            "vm_persistent_actor_performance_rejects_empty_fixture_name_and_workload",
            "vm_persistent_actor_performance_rejects_invalid_compaction_count",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark",
            "vm_distributed_storage_reports_finalize_and_partial_write_failures",
            "vm_distributed_storage_rejects_stale_snapshot_replay",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_state_test.rs",
        &["vm_distributed_state_exports_and_imports_deterministic_snapshots"],
    ),
    (
        "crates/terlan/src/runtime/vm/process_test.rs",
        &[
            "process_selective_receive_preserves_large_skipped_mailbox_prefix",
            "process_exit_clears_mailbox_and_returns_resource_handles",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/timer_test.rs",
        &["timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order"],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-performance-budget-check: vm-persistent-actor-adapter-conformance-check",
    "vm_persistent_actor_performance_estimates_small_actor_budget",
    "vm_persistent_actor_performance_scales_event_storm_above_small_actor",
    "vm_persistent_actor_performance_compaction_reduces_replay_budget",
    "vm_persistent_actor_performance_rejects_empty_fixture_name_and_workload",
    "vm_persistent_actor_performance_rejects_invalid_compaction_count",
    "vm-persistent-actor-runtime-baseline",
    "vm_persistent_actor_performance_test",
    "vm-persistent-actor-performance",
];

const FIXTURE_BUDGETS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "small actor append",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    (
        "large actor snapshot",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    ("high-event replay", "p50", "p95", "p99", "pending baseline"),
    (
        "mailbox-heavy checkpoint",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    (
        "timer-heavy recovery",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    (
        "post-compaction replay",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    (
        "cross-adapter restore",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
    (
        "cold-start recovery",
        "p50",
        "p95",
        "p99",
        "pending baseline",
    ),
];

const DETERMINISTIC_BASELINE_ESTIMATES: &[&str] = &[
    "small actor deterministic budget estimate",
    "event storm deterministic budget estimate",
    "post-compaction replay budget reduction estimate",
    "invalid empty workload budget rejection",
    "invalid compaction count budget rejection",
];

const TIMING_BREAKDOWNS: &[&str] = &[
    "scheduler time",
    "serialization time",
    "adapter I/O time",
    "schema migration time",
    "compaction time",
    "VM replay time",
];

const SIZE_BUDGETS: &[&str] = &[
    "memory bytes per actor generation",
    "disk bytes per checkpoint generation",
    "replay bytes before and after compaction",
    "resource snapshot bytes",
    "mailbox checkpoint bytes",
];

const ADVERSARIAL_PERFORMANCE_CASES: &[&str] = &[
    "event storm",
    "snapshot storm",
    "slow adapter",
    "large mailbox checkpoint",
    "many durable resources",
    "compaction under load",
    "large export restore",
    "pathological schema migration chain",
];

const REJECTED_BUDGET_PATHS: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data describing vm persistent actor performance summary.
pub struct VmPersistentActorPerformanceSummary {
    pub fixture_budget_count: usize,
    pub deterministic_baseline_estimate_count: usize,
    pub timing_breakdown_count: usize,
    pub adversarial_performance_case_count: usize,
    pub rejected_budget_path_count: usize,
    pub measured_runtime_baseline_count: usize,
    pub report_path: PathBuf,
}

/// Runs vm persistent actor performance.
pub fn run_vm_persistent_actor_performance(
    root: &Path,
) -> QualityResult<VmPersistentActorPerformanceSummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor performance foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor performance fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    let measured_runtime_baseline = load_measured_runtime_baseline(root, &mut diagnostics)?;
    let baseline_comparison =
        compare_measured_runtime_baseline(root, &measured_runtime_baseline, &mut diagnostics)?;
    if !diagnostics.is_empty() {
        return Err(render_failure(
            "vm-persistent-actor-performance",
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
    let fixture_budgets: Vec<_> = FIXTURE_BUDGETS
        .iter()
        .map(|(fixture, p50, p95, p99, state)| {
            json!({
                "fixture": fixture,
                "p50": p50,
                "p95": p95,
                "p99": p99,
                "state": state,
                "pass": false
            })
        })
        .collect();
    let report = json!({
        "schema": "terlan-vm-persistent-actor-performance-report-v1",
        "fixtureBudgets": fixture_budgets,
        "deterministicBaselineEstimates": DETERMINISTIC_BASELINE_ESTIMATES,
        "timingBreakdowns": TIMING_BREAKDOWNS,
        "sizeBudgets": SIZE_BUDGETS,
        "adversarialPerformanceCases": ADVERSARIAL_PERFORMANCE_CASES,
        "rejectedBudgetPaths": REJECTED_BUDGET_PATHS,
        "measuredRuntimeBaseline": measured_runtime_baseline,
        "baselineComparison": baseline_comparison
    });
    let report_text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize VM persistent actor performance report: {err}")
    })?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorPerformanceSummary {
        fixture_budget_count: FIXTURE_BUDGETS.len(),
        deterministic_baseline_estimate_count: DETERMINISTIC_BASELINE_ESTIMATES.len(),
        timing_breakdown_count: TIMING_BREAKDOWNS.len(),
        adversarial_performance_case_count: ADVERSARIAL_PERFORMANCE_CASES.len(),
        rejected_budget_path_count: REJECTED_BUDGET_PATHS.len(),
        measured_runtime_baseline_count: 1,
        report_path,
    })
}

fn load_measured_runtime_baseline(
    root: &Path,
    diagnostics: &mut Vec<String>,
) -> QualityResult<serde_json::Value> {
    let path = root.join(BENCHMARK_REPORT_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{BENCHMARK_REPORT_PATH}: failed to read benchmark: {err}"))?;
    let report: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("{BENCHMARK_REPORT_PATH}: invalid JSON: {err}"))?;
    if report["schema"] != "terlan.vm-persistent-actor-benchmark.v1" {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: invalid persistent actor benchmark schema"
        ));
    }
    if report["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: benchmark must verify replay correctness"
        ));
    }
    let runs = report["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: benchmark requires at least three measured runs"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_measured_runtime_row(row, &format!("run {index}"), diagnostics);
    }
    validate_measured_runtime_row(&report["aggregate"], "aggregate", diagnostics);
    validate_file_backed_runtime(&report["file_backed"], diagnostics);
    validate_compaction_runtime(&report["compaction"], diagnostics);
    validate_schema_migration_runtime(&report["schema_migration"], diagnostics);
    validate_cross_adapter_restore_runtime(&report["cross_adapter_restore"], diagnostics);
    validate_scheduler_attribution_runtime(&report["scheduler_attribution"], diagnostics);
    validate_memory_high_water_runtime(&report["memory_high_water"], diagnostics);
    Ok(report)
}

fn validate_memory_high_water_runtime(memory: &serde_json::Value, diagnostics: &mut Vec<String>) {
    if memory["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water benchmark must verify account and release metrics"
        ));
    }
    if memory["budget_pass"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water benchmark must pass its hard budget"
        ));
    }
    let run_count = memory["run_count"].as_u64();
    let samples_per_run = memory["samples_per_run"].as_u64();
    if run_count.is_none_or(|runs| runs < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water benchmark requires at least three runs"
        ));
    }
    if samples_per_run.is_none_or(|samples| samples < 10) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water benchmark requires at least ten samples/run"
        ));
    }
    if memory["events_per_sample"]
        .as_u64()
        .is_none_or(|events| events == 0)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water benchmark requires a non-empty event workload"
        ));
    }
    let soft_limit = memory["soft_limit_bytes"].as_u64();
    let hard_limit = memory["hard_limit_bytes"].as_u64();
    if !matches!((soft_limit, hard_limit), (Some(soft), Some(hard)) if soft > 0 && soft <= hard) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water limits must be positive and ordered"
        ));
    }
    let runs = memory["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water report requires three run rows"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_memory_high_water_row(
            row,
            hard_limit,
            &format!("memory high-water run {index}"),
            diagnostics,
        );
    }
    let expected_aggregate_samples = run_count
        .zip(samples_per_run)
        .and_then(|(runs, samples)| runs.checked_mul(samples));
    validate_memory_high_water_row(
        &memory["aggregate"],
        hard_limit,
        "memory high-water aggregate",
        diagnostics,
    );
    if memory["aggregate"]["sample_count"].as_u64() != expected_aggregate_samples {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: memory high-water aggregate must cover every run"
        ));
    }
}

fn validate_memory_high_water_row(
    row: &serde_json::Value,
    hard_limit: Option<u64>,
    label: &str,
    diagnostics: &mut Vec<String>,
) {
    let p50 = row["p50_bytes"].as_u64();
    let p95 = row["p95_bytes"].as_u64();
    let p99 = row["p99_bytes"].as_u64();
    if row["sample_count"].as_u64().is_none_or(|count| count < 10) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} must contain at least ten samples"
        ));
    }
    if !matches!((p50, p95, p99), (Some(p50), Some(p95), Some(p99)) if p50 > 0 && p50 <= p95 && p95 <= p99)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} byte percentiles must be positive and monotonic"
        ));
    }
    if !matches!((p99, hard_limit), (Some(p99), Some(hard)) if p99 <= hard) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} p99 must not exceed the hard memory budget"
        ));
    }
}

fn validate_scheduler_attribution_runtime(
    scheduler: &serde_json::Value,
    diagnostics: &mut Vec<String>,
) {
    if scheduler["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler attribution must verify VM tick and reduction accounting"
        ));
    }
    let run_count = scheduler["run_count"].as_u64();
    let samples_per_run = scheduler["samples_per_run"].as_u64();
    if run_count.is_none_or(|runs| runs < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler attribution requires at least three runs"
        ));
    }
    if samples_per_run.is_none_or(|samples| samples < 10) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler attribution requires at least ten samples/run"
        ));
    }
    let events = scheduler["events_per_sample"].as_u64();
    let reductions = scheduler["reductions_per_sample"].as_u64();
    if !matches!((events, reductions), (Some(events), Some(reductions)) if events > 0 && events.checked_add(2) == Some(reductions))
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler reductions/sample must cover snapshot, events, and replay"
        ));
    }
    let runs = scheduler["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler attribution report requires three run rows"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_scheduler_attribution_row(
            row,
            reductions,
            &format!("scheduler attribution run {index}"),
            diagnostics,
        );
    }
    let expected_aggregate_samples = run_count
        .zip(samples_per_run)
        .and_then(|(runs, samples)| runs.checked_mul(samples));
    validate_scheduler_attribution_row(
        &scheduler["aggregate"],
        reductions,
        "scheduler attribution aggregate",
        diagnostics,
    );
    if scheduler["aggregate"]["sample_count"].as_u64() != expected_aggregate_samples {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: scheduler aggregate sample count must cover every run"
        ));
    }
}

fn validate_scheduler_attribution_row(
    row: &serde_json::Value,
    reductions_per_sample: Option<u64>,
    label: &str,
    diagnostics: &mut Vec<String>,
) {
    let samples = row["sample_count"].as_u64();
    let ticks = row["scheduler_ticks"].as_u64();
    if !matches!((samples, ticks), (Some(samples), Some(ticks)) if samples > 0 && ticks == samples)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} must attribute one scheduler tick per sample"
        ));
    }
    let expected_reductions = samples
        .zip(reductions_per_sample)
        .and_then(|(samples, reductions)| samples.checked_mul(reductions));
    if row["reductions_charged"].as_u64() != expected_reductions {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} reductions must match the scheduled workload"
        ));
    }
    validate_phase_latency(
        &row["scheduler_overhead"],
        &format!("{label} scheduler overhead"),
        diagnostics,
    );
}

fn validate_cross_adapter_restore_runtime(
    restore: &serde_json::Value,
    diagnostics: &mut Vec<String>,
) {
    if restore["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore benchmark must verify destination replay correctness"
        ));
    }
    if restore["run_count"].as_u64().is_none_or(|runs| runs < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore benchmark requires at least three runs"
        ));
    }
    if restore["samples_per_run"]
        .as_u64()
        .is_none_or(|samples| samples < 10)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore benchmark requires at least ten samples/run"
        ));
    }
    let source = restore["source_adapter"].as_str();
    let destination = restore["destination_adapter"].as_str();
    if !matches!((source, destination), (Some(source), Some(destination)) if !source.is_empty() && !destination.is_empty() && source != destination)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore requires distinct non-empty source and destination adapters"
        ));
    }
    if restore["events_per_restore"]
        .as_u64()
        .is_none_or(|events| events == 0)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore must replay at least one event"
        ));
    }
    let runs = restore["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: cross-adapter restore report requires three phase rows"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_phase_latency(
            row,
            &format!("cross-adapter restore run {index}"),
            diagnostics,
        );
    }
    validate_phase_latency(
        &restore["aggregate"],
        "cross-adapter restore aggregate",
        diagnostics,
    );
}

fn validate_schema_migration_runtime(migration: &serde_json::Value, diagnostics: &mut Vec<String>) {
    if migration["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: schema migration benchmark must verify ordered chain correctness"
        ));
    }
    if migration["run_count"].as_u64().is_none_or(|runs| runs < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: schema migration benchmark requires at least three runs"
        ));
    }
    if migration["samples_per_run"]
        .as_u64()
        .is_none_or(|samples| samples < 10)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: schema migration benchmark requires at least ten samples/run"
        ));
    }
    let schema_versions = migration["schema_versions"].as_u64();
    let planned_edges = migration["planned_edges"].as_u64();
    if !matches!((schema_versions, planned_edges), (Some(versions), Some(edges)) if versions >= 2 && edges == versions - 1)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: schema migration must plan one ordered edge between every schema version"
        ));
    }
    let runs = migration["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: schema migration report requires three phase rows"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_phase_latency(row, &format!("schema migration run {index}"), diagnostics);
    }
    validate_phase_latency(
        &migration["aggregate"],
        "schema migration aggregate",
        diagnostics,
    );
}

fn validate_compaction_runtime(compaction: &serde_json::Value, diagnostics: &mut Vec<String>) {
    if compaction["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: compaction benchmark must verify retained suffix correctness"
        ));
    }
    if compaction["run_count"].as_u64().is_none_or(|runs| runs < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: compaction benchmark requires at least three runs"
        ));
    }
    if compaction["samples_per_run"]
        .as_u64()
        .is_none_or(|samples| samples < 10)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: compaction benchmark requires at least ten samples/run"
        ));
    }
    let events_before = compaction["events_before"].as_u64();
    let events_retained = compaction["events_retained"].as_u64();
    if !matches!((events_before, events_retained), (Some(before), Some(retained)) if retained > 0 && retained < before)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: compaction must retain a non-empty proper event suffix"
        ));
    }
    let runs = compaction["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: compaction report requires three phase rows"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_phase_latency(row, &format!("compaction run {index}"), diagnostics);
    }
    validate_phase_latency(
        &compaction["aggregate"],
        "compaction aggregate",
        diagnostics,
    );
}

fn validate_file_backed_runtime(file_backed: &serde_json::Value, diagnostics: &mut Vec<String>) {
    if file_backed["correctness_verified"] != true {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: file-backed benchmark must verify durable replay"
        ));
    }
    let runs = file_backed["runs"].as_array();
    if runs.is_none_or(|runs| runs.len() < 3) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: file-backed benchmark requires at least three runs"
        ));
    }
    for (index, row) in runs.into_iter().flatten().enumerate() {
        validate_file_backed_runtime_row(row, &format!("file-backed run {index}"), diagnostics);
    }
    validate_file_backed_runtime_row(
        &file_backed["aggregate"],
        "file-backed aggregate",
        diagnostics,
    );
}

fn validate_file_backed_runtime_row(
    row: &serde_json::Value,
    label: &str,
    diagnostics: &mut Vec<String>,
) {
    if row["sample_count"].as_u64().is_none_or(|count| count < 10) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} must contain at least ten samples"
        ));
    }
    for phase in [
        "snapshot_commit",
        "append_events",
        "reopen_load",
        "vm_replay",
        "reopen_replay",
    ] {
        validate_phase_latency(&row[phase], &format!("{label} `{phase}`"), diagnostics);
    }
    let disk_p50 = row["disk_bytes_p50"].as_u64();
    let disk_p99 = row["disk_bytes_p99"].as_u64();
    if !matches!((disk_p50, disk_p99), (Some(p50), Some(p99)) if p50 > 0 && p50 <= p99) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} disk bytes must be positive and monotonic"
        ));
    }
    let reopen_p99 = row["reopen_load"]["p99_ns"].as_u64().unwrap_or(u64::MAX);
    let replay_p99 = row["vm_replay"]["p99_ns"].as_u64().unwrap_or(u64::MAX);
    let combined_p99 = row["reopen_replay"]["p99_ns"].as_u64().unwrap_or(0);
    if combined_p99 < reopen_p99 || combined_p99 < replay_p99 {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} combined reopen/replay p99 must cover both attributed phases"
        ));
    }
}

fn validate_phase_latency(row: &serde_json::Value, label: &str, diagnostics: &mut Vec<String>) {
    let p50 = row["p50_ns"].as_u64();
    let p95 = row["p95_ns"].as_u64();
    let p99 = row["p99_ns"].as_u64();
    if !matches!((p50, p95, p99), (Some(p50), Some(p95), Some(p99)) if p50 > 0 && p50 <= p95 && p95 <= p99)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} percentiles must be positive and monotonic"
        ));
    }
}

fn compare_measured_runtime_baseline(
    root: &Path,
    measured: &serde_json::Value,
    diagnostics: &mut Vec<String>,
) -> QualityResult<serde_json::Value> {
    let text = fs::read_to_string(root.join(BASELINE_PATH))
        .map_err(|err| format!("{BASELINE_PATH}: failed to read baseline: {err}"))?;
    let baseline: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("{BASELINE_PATH}: invalid JSON: {err}"))?;
    if baseline["schema"] != "terlan.vm-persistent-actor-budget.v1" {
        diagnostics.push(format!("{BASELINE_PATH}: invalid baseline schema"));
    }
    for field in ["benchmark", "adapter", "events_per_sample"] {
        if baseline[field] != measured[field] {
            diagnostics.push(format!(
                "{BASELINE_PATH}: `{field}` does not match measured benchmark"
            ));
        }
    }
    let observed_runs = measured["run_count"].as_u64().unwrap_or(0);
    let observed_samples = measured["samples_per_run"].as_u64().unwrap_or(0);
    let observed_p99 = measured["aggregate"]["p99_ns"].as_u64().unwrap_or(u64::MAX);
    let observed_throughput = measured["aggregate"]["throughput_events_per_second"]
        .as_u64()
        .unwrap_or(0);
    let required_runs = baseline["required_run_count"].as_u64().unwrap_or(u64::MAX);
    let minimum_samples = baseline["minimum_samples_per_run"]
        .as_u64()
        .unwrap_or(u64::MAX);
    let maximum_p99 = baseline["maximum_p99_ns"].as_u64().unwrap_or(0);
    let minimum_throughput = baseline["minimum_throughput_events_per_second"]
        .as_u64()
        .unwrap_or(u64::MAX);
    let run_count_pass = observed_runs >= required_runs;
    let sample_count_pass = observed_samples >= minimum_samples;
    let p99_pass = observed_p99 <= maximum_p99;
    let throughput_pass = observed_throughput >= minimum_throughput;
    if !run_count_pass {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {observed_runs} runs is below baseline {required_runs}"
        ));
    }
    if !sample_count_pass {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {observed_samples} samples/run is below baseline {minimum_samples}"
        ));
    }
    if !p99_pass {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: p99 {observed_p99} ns exceeds baseline {maximum_p99} ns"
        ));
    }
    if !throughput_pass {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: throughput {observed_throughput} events/sec is below baseline {minimum_throughput}"
        ));
    }
    Ok(json!({
        "baselinePath": BASELINE_PATH,
        "runCount": {"observed": observed_runs, "required": required_runs, "pass": run_count_pass},
        "samplesPerRun": {"observed": observed_samples, "minimum": minimum_samples, "pass": sample_count_pass},
        "p99Ns": {"observed": observed_p99, "maximum": maximum_p99, "pass": p99_pass},
        "throughputEventsPerSecond": {"observed": observed_throughput, "minimum": minimum_throughput, "pass": throughput_pass},
        "pass": run_count_pass && sample_count_pass && p99_pass && throughput_pass
    }))
}

fn validate_measured_runtime_row(
    row: &serde_json::Value,
    label: &str,
    diagnostics: &mut Vec<String>,
) {
    let p50 = row["p50_ns"].as_u64();
    let p95 = row["p95_ns"].as_u64();
    let p99 = row["p99_ns"].as_u64();
    if !matches!((p50, p95, p99), (Some(p50), Some(p95), Some(p99)) if p50 > 0 && p50 <= p95 && p95 <= p99)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} latency percentiles must be positive and monotonic"
        ));
    }
    if row["sample_count"].as_u64().is_none_or(|count| count < 10) {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} must contain at least ten samples"
        ));
    }
    if row["throughput_events_per_second"]
        .as_u64()
        .is_none_or(|throughput| throughput == 0)
    {
        diagnostics.push(format!(
            "{BENCHMARK_REPORT_PATH}: {label} throughput must be positive"
        ));
    }
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
    let text = fs::read_to_string(root.join("Makefile")).map_err(|err| {
        format!("Makefile: failed to read persistent actor performance gate: {err}")
    })?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor performance gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    let fixture_entries = fixture_budget_report_entries();
    diagnostics.extend(validate_entries_for_placeholder_terms(
        "fixture budgets",
        &fixture_entries,
    ));
    for (label, entries) in [
        (
            "deterministic baseline estimates",
            DETERMINISTIC_BASELINE_ESTIMATES,
        ),
        ("timing breakdowns", TIMING_BREAKDOWNS),
        ("size budgets", SIZE_BUDGETS),
        (
            "adversarial performance cases",
            ADVERSARIAL_PERFORMANCE_CASES,
        ),
        ("rejected budget paths", REJECTED_BUDGET_PATHS),
    ] {
        diagnostics.extend(validate_entries_for_placeholder_terms(label, entries));
    }
    diagnostics
}

fn fixture_budget_report_entries() -> Vec<&'static str> {
    FIXTURE_BUDGETS
        .iter()
        .flat_map(|(fixture, p50, p95, p99, state)| [*fixture, *p50, *p95, *p99, *state])
        .collect()
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| {
            let lower = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .filter(move |term| lower.contains(**term))
                .map(move |term| {
                    format!(
                        "VM persistent actor performance {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
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
#[path = "vm_persistent_actor_performance_test.rs"]
#[cfg(test)]
mod vm_persistent_actor_performance_test;
