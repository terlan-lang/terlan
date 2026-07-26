//! Comparable checked-CoreIR and native-AOT HTTP benchmark harness.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use super::hardware::{sha256, HardwareFingerprint};
use serde::{Deserialize, Serialize};

#[path = "http_aot_performance/deltas.rs"]
mod deltas;

#[path = "http_aot_performance/benchmark_evidence.rs"]
mod benchmark_evidence;

#[path = "http_aot_performance/additional_workloads.rs"]
mod additional_workloads;

#[path = "http_aot_performance/harness.rs"]
mod harness;

#[path = "http_client.rs"]
mod http_client;

#[path = "http_benchmark_support.rs"]
mod http_benchmark_support;

#[path = "http_aot_performance/policy.rs"]
mod policy;

#[path = "http_aot_performance/report_io.rs"]
mod report_io;

use harness::{
    create_workspace, measure_request_rounds, measure_requests, median_throughput_round,
    process_memory_snapshot, reserve_port, resident_bytes, spawn_server, wait_for_generation,
    write_handler_source, write_package, ServerGuard,
};
use report_io::{
    parse_report, read_file, read_nonnegative_env, read_positive_env, sha256_file,
    unix_timestamp_nanos, unix_timestamp_seconds, write_json,
};

/// Benchmark command that records one executable HTTP lane.
pub(crate) const COMMAND: &str = "http-aot-performance";
/// Benchmark command that compares two previously recorded lanes.
pub(crate) const COMPARE_COMMAND: &str = "http-aot-performance-compare";
/// Pure report-contract self-test command used when socket benchmarks are unavailable.
pub(crate) const SELF_TEST_COMMAND: &str = "http-aot-performance-self-test";

const REPORT_SCHEMA: &str = "terlan-http-aot-performance-v2";
const LEGACY_CHECKED_COREIR_SCHEMA: &str = "terlan-http-aot-performance-v1";
const COMPARISON_SCHEMA: &str = "terlan-http-aot-performance-comparison-v2";
const DEFAULT_NATIVE_OUTPUT: &str = "target/quality/http-native-aot-performance.json";
const DEFAULT_CHECKED_COREIR_OUTPUT: &str =
    "../benchmarks/results/http-checked-coreir-performance.json";
const DEFAULT_COMPARISON_OUTPUT: &str = "target/quality/http-aot-performance-comparison.json";
const DEFAULT_POLICY: &str = "benchmarks/baselines/http-aot-performance-limits.json";

/// Runtime implementation measured by one report.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum HttpExecutionLane {
    /// Preserved checked-CoreIR interpreter reference.
    CheckedCoreir,
    /// Current native image handler runtime.
    NativeAot,
}

impl HttpExecutionLane {
    /// Parses the stable environment spelling for a runtime lane.
    fn from_env() -> Result<Self, String> {
        match env::var("TERLAN_BENCH_HTTP_AOT_LANE")
            .unwrap_or_else(|_| "native-aot".to_string())
            .as_str()
        {
            "checked-coreir" => Ok(Self::CheckedCoreir),
            "native-aot" => Ok(Self::NativeAot),
            value => Err(format!(
                "TERLAN_BENCH_HTTP_AOT_LANE expects `checked-coreir` or `native-aot`, got `{value}`"
            )),
        }
    }

    /// Returns the default report path for this lane.
    fn default_output(self) -> &'static str {
        match self {
            Self::CheckedCoreir => DEFAULT_CHECKED_COREIR_OUTPUT,
            Self::NativeAot => DEFAULT_NATIVE_OUTPUT,
        }
    }
}

/// Fixed workload dimensions shared by both executable lanes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct HttpPerformanceWorkload {
    #[serde(default)]
    warmup_requests: usize,
    #[serde(default)]
    measurement_rounds: usize,
    #[serde(alias = "connection_workers")]
    #[serde(default)]
    readiness_reactors: usize,
    sequential_requests: usize,
    concurrency: usize,
    requests_per_worker: usize,
    longevity_requests: usize,
    payload_bytes: usize,
    #[serde(default)]
    measurement_duration_ms: u64,
    #[serde(default)]
    soak_seconds: u64,
}

impl HttpPerformanceWorkload {
    /// Reads benchmark workload controls while retaining non-zero defaults.
    fn from_env() -> Self {
        let readiness_reactors = read_positive_env(
            "TERLAN_BENCH_HTTP_AOT_REACTORS",
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .clamp(1, 32),
        )
        .min(32);
        Self {
            // Warm the compiler-produced image, dynamic linker, allocator,
            // TCP path, every owner, and CPU frequency state before any
            // measured request enters a report.
            warmup_requests: read_positive_env(
                "TERLAN_BENCH_HTTP_AOT_WARMUP",
                readiness_reactors.saturating_mul(800),
            ),
            // A median of independent rounds rejects one-off scheduler and
            // frequency-state noise while retaining every raw round.
            measurement_rounds: read_positive_env("TERLAN_BENCH_HTTP_AOT_ROUNDS", 5),
            // Keep socket-readiness ownership explicit and reproducible instead
            // of inheriting an implicit host topology in benchmark subprocesses.
            readiness_reactors,
            // Keep p99 distinct from the single maximum request so scheduler
            // noise cannot decide the regression gate by itself.
            sequential_requests: read_positive_env("TERLAN_BENCH_HTTP_AOT_ITERATIONS", 500),
            concurrency: read_positive_env("TERLAN_BENCH_HTTP_AOT_CONCURRENCY", 8),
            // Eight hundred pressure samples keep p99 from collapsing onto a
            // couple of scheduler outliers under concurrent connection load.
            requests_per_worker: read_positive_env(
                "TERLAN_BENCH_HTTP_AOT_REQUESTS_PER_WORKER",
                100,
            ),
            longevity_requests: read_positive_env("TERLAN_BENCH_HTTP_AOT_LONGEVITY", 1_000),
            payload_bytes: read_positive_env("TERLAN_BENCH_HTTP_AOT_PAYLOAD_BYTES", 512),
            measurement_duration_ms: read_nonnegative_env("TERLAN_BENCH_HTTP_DURATION_MS", 0),
            soak_seconds: read_nonnegative_env("TERLAN_BENCH_HTTP_SOAK_SECONDS", 0),
        }
    }
}

/// Percentile and throughput summary for one request track.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpTiming {
    sample_count: usize,
    total_wall_ns: u128,
    throughput_requests_per_second: u128,
    min_ns: u128,
    mean_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

/// Auditable raw rounds behind each median timing selected for policy checks.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct HttpMeasurementEvidence {
    aggregation: String,
    sequential_rounds: Vec<HttpTiming>,
    pressure_rounds: Vec<HttpTiming>,
    longevity_rounds: Vec<HttpTiming>,
}

/// Process resident-memory observations around the measured workload.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpAllocationEvidence {
    measurement: String,
    before_bytes: Option<u64>,
    after_pressure_bytes: Option<u64>,
    after_longevity_bytes: Option<u64>,
    peak_observed_bytes: Option<u64>,
    #[serde(default)]
    snapshots: Vec<HttpProcessMemorySnapshot>,
}

/// Procfs attribution captured at stable benchmark lifecycle boundaries.
type HttpProcessMemorySnapshot = http_client::ProcessMemorySnapshot;

/// Concurrent queue-pressure evidence for one server generation.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpPressureEvidence {
    concurrency: usize,
    attempted_requests: usize,
    completed_requests: usize,
    failed_requests: usize,
    timing: HttpTiming,
}

/// Evidence that one server remains correct over a sustained request run.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpLongevityEvidence {
    attempted_requests: usize,
    completed_requests: usize,
    timing: HttpTiming,
}

/// Evidence that source replacement publishes and serves a second generation.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpGenerationEvidence {
    generations_observed: Vec<String>,
    reload_latency_ns: u128,
    first_generation_body_verified: bool,
    second_generation_body_verified: bool,
    in_flight_lifetime_gate: String,
}

/// Additional workload shape used to expose costs hidden by the primary lane.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpNamedWorkloadEvidence {
    name: String,
    connection_mode: String,
    concurrency: usize,
    requests: usize,
    payload_bytes: usize,
    timing: HttpTiming,
    #[serde(default)]
    rounds: Vec<HttpTiming>,
}

/// Complete executable report for one HTTP runtime lane.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpPerformanceReport {
    schema: String,
    status: String,
    lane: HttpExecutionLane,
    timestamp_unix_seconds: u64,
    compiler_binary_sha256: String,
    hardware: HardwareFingerprint,
    workload: HttpPerformanceWorkload,
    #[serde(default)]
    measurement: HttpMeasurementEvidence,
    sequential: HttpTiming,
    allocation: HttpAllocationEvidence,
    pressure: HttpPressureEvidence,
    longevity: HttpLongevityEvidence,
    generation_overlap: HttpGenerationEvidence,
    #[serde(default)]
    additional_workloads: Vec<HttpNamedWorkloadEvidence>,
    #[serde(default)]
    benchmark_evidence: Option<benchmark_evidence::HttpExtendedBenchmarkEvidence>,
}

/// Comparison deltas between the checked reference and native AOT lane.
#[derive(Clone, Debug, Serialize)]
struct HttpPerformanceComparison {
    schema: &'static str,
    status: &'static str,
    hardware_fingerprint_sha256: String,
    workload: HttpPerformanceWorkload,
    checked_coreir_report_sha256: String,
    native_aot_report_sha256: String,
    performance_policy_sha256: String,
    performance_policy: policy::HttpPerformancePolicy,
    checked_coreir: HttpPerformanceReport,
    native_aot: HttpPerformanceReport,
    deltas: serde_json::Value,
}

/// Runs one configured executable HTTP benchmark lane and writes its report.
pub(crate) fn run_cli() -> ExitCode {
    match run_lane_from_env() {
        Ok(path) => {
            println!("[{COMMAND}] completed; wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[{COMMAND}] failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Compares checked-CoreIR and native-AOT reports from the same machine.
pub(crate) fn run_compare_cli() -> ExitCode {
    match compare_from_env() {
        Ok(path) => {
            println!("[{COMPARE_COMMAND}] completed; wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[{COMPARE_COMMAND}] failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Exercises report statistics and rejection rules without opening a socket.
pub(crate) fn run_self_test_cli() -> ExitCode {
    match run_self_test() {
        Ok(()) => {
            println!("[{SELF_TEST_COMMAND}] completed");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("[{SELF_TEST_COMMAND}] failed: {error}");
            ExitCode::from(1)
        }
    }
}

/// Runs deterministic benchmark-report checks shared with unit coverage.
fn run_self_test() -> Result<(), String> {
    report_io::self_test_legacy_adapter()?;
    let timing = HttpTiming::from_durations(
        &[
            Duration::from_nanos(10),
            Duration::from_nanos(20),
            Duration::from_nanos(40),
        ],
        Duration::from_nanos(100),
    )?;
    if timing.p50_ns != 20
        || timing.p95_ns != 40
        || timing.p99_ns != 40
        || timing.throughput_requests_per_second != 30_000_000
    {
        return Err("tail latency or throughput summary changed".to_string());
    }
    let comparison = compare_reports(
        fixture_report(HttpExecutionLane::CheckedCoreir),
        fixture_report(HttpExecutionLane::NativeAot),
        "checked".to_string(),
        "native".to_string(),
    )?;
    if comparison.hardware_fingerprint_sha256 != "hardware" {
        return Err("matching hardware comparison changed".to_string());
    }
    let checked = fixture_report(HttpExecutionLane::CheckedCoreir);
    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.hardware.sha256 = "different".to_string();
    if compare_reports(checked, native, "checked".to_string(), "native".to_string()).is_ok() {
        return Err("mixed hardware reports were accepted".to_string());
    }
    let checked = fixture_report(HttpExecutionLane::CheckedCoreir);
    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    native.pressure.completed_requests = 1;
    if compare_reports(checked, native, "checked".to_string(), "native".to_string()).is_ok() {
        return Err("incomplete pressure evidence was accepted".to_string());
    }
    expect_budget_rejection("sequential_p50", |report| report.sequential.p50_ns *= 2)?;
    expect_budget_rejection("sequential_p95", |report| report.sequential.p95_ns *= 2)?;
    expect_budget_rejection("sequential_p99", |report| report.sequential.p99_ns *= 2)?;
    expect_budget_rejection("sequential_throughput", |report| {
        report.sequential.throughput_requests_per_second /= 2;
    })?;
    expect_budget_rejection("pressure_p50", |report| {
        report.pressure.timing.p50_ns *= 2;
    })?;
    expect_budget_rejection("pressure_p95", |report| {
        report.pressure.timing.p95_ns *= 2;
    })?;
    expect_budget_rejection("pressure_p99", |report| {
        report.pressure.timing.p99_ns *= 2;
    })?;
    expect_budget_rejection("pressure_throughput", |report| {
        report.pressure.timing.throughput_requests_per_second /= 2;
    })?;
    expect_budget_rejection("longevity_p50", |report| {
        report.longevity.timing.p50_ns *= 2;
    })?;
    expect_budget_rejection("longevity_p95", |report| {
        report.longevity.timing.p95_ns *= 2;
    })?;
    expect_budget_rejection("longevity_p99", |report| {
        report.longevity.timing.p99_ns *= 2;
    })?;
    expect_budget_rejection("longevity_throughput", |report| {
        report.longevity.timing.throughput_requests_per_second /= 2;
    })?;
    expect_budget_rejection("peak_rss", |report| {
        report.allocation.peak_observed_bytes = Some(3);
    })?;
    expect_budget_rejection("generation_reload", |report| {
        report.generation_overlap.reload_latency_ns *= 2;
    })?;
    let mut weakened_policy = policy::canonical_policy()?;
    weakened_policy.maximum_sequential_p99_ratio = 1.51;
    if policy::validate_policy(&weakened_policy).is_ok() {
        return Err("weakened performance policy was accepted".to_string());
    }
    Ok(())
}

/// Requires one mutated native report to fail its named quantitative limit.
fn expect_budget_rejection(
    dimension: &str,
    mutate: impl FnOnce(&mut HttpPerformanceReport),
) -> Result<(), String> {
    let checked = fixture_report(HttpExecutionLane::CheckedCoreir);
    let mut native = fixture_report(HttpExecutionLane::NativeAot);
    mutate(&mut native);
    let error = policy::validate_performance(&checked, &native, &policy::canonical_policy()?)
        .expect_err("over-budget performance evidence must be rejected");
    if !error.contains(dimension) {
        return Err(format!("{dimension} performance budget diagnostic changed"));
    }
    Ok(())
}

/// Builds one complete deterministic report for contract verification.
fn fixture_report(lane: HttpExecutionLane) -> HttpPerformanceReport {
    let timing = HttpTiming::from_durations(
        &[Duration::from_nanos(10), Duration::from_nanos(20)],
        Duration::from_nanos(30),
    )
    .expect("fixed non-empty timing fixture");
    HttpPerformanceReport {
        schema: REPORT_SCHEMA.to_string(),
        status: "completed".to_string(),
        lane,
        timestamp_unix_seconds: 1,
        compiler_binary_sha256: "compiler".to_string(),
        hardware: HardwareFingerprint {
            schema: "terlan-benchmark-hardware-v1".to_string(),
            operating_system: "linux".to_string(),
            architecture: "x86_64".to_string(),
            cpu_model: "test".to_string(),
            logical_cpu_count: 2,
            rustc_version: "rustc test".to_string(),
            sha256: "hardware".to_string(),
        },
        workload: HttpPerformanceWorkload {
            warmup_requests: 1,
            measurement_rounds: 1,
            readiness_reactors: 2,
            sequential_requests: 2,
            concurrency: 2,
            requests_per_worker: 1,
            longevity_requests: 2,
            payload_bytes: 8,
            measurement_duration_ms: 0,
            soak_seconds: 0,
        },
        measurement: HttpMeasurementEvidence {
            aggregation: "median-throughput-round".to_string(),
            sequential_rounds: vec![timing.clone()],
            pressure_rounds: vec![timing.clone()],
            longevity_rounds: vec![timing.clone()],
        },
        sequential: timing.clone(),
        allocation: HttpAllocationEvidence {
            measurement: "server_process_resident_set_bytes".to_string(),
            before_bytes: Some(1),
            after_pressure_bytes: Some(2),
            after_longevity_bytes: Some(2),
            peak_observed_bytes: Some(2),
            snapshots: Vec::new(),
        },
        pressure: HttpPressureEvidence {
            concurrency: 2,
            attempted_requests: 2,
            completed_requests: 2,
            failed_requests: 0,
            timing: timing.clone(),
        },
        longevity: HttpLongevityEvidence {
            attempted_requests: 2,
            completed_requests: 2,
            timing,
        },
        generation_overlap: HttpGenerationEvidence {
            generations_observed: vec!["one".to_string(), "two".to_string()],
            reload_latency_ns: 10,
            first_generation_body_verified: true,
            second_generation_body_verified: true,
            in_flight_lifetime_gate: "make gate".to_string(),
        },
        additional_workloads: Vec::new(),
        benchmark_evidence: Some(benchmark_evidence::fixture()),
    }
}

/// Resolves environment settings and records one lane.
fn run_lane_from_env() -> Result<PathBuf, String> {
    let lane = HttpExecutionLane::from_env()?;
    let output = env::var_os("TERLAN_BENCH_HTTP_AOT_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(lane.default_output()));
    let compiler = env::var_os("TERLAN_BENCH_HTTP_AOT_TERLC_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/release/terlc"));
    let report = run_lane(lane, &compiler, HttpPerformanceWorkload::from_env())?;
    validate_report(&report, lane)?;
    write_json(&output, &report)?;
    Ok(output)
}

/// Executes the full HTTP lifecycle benchmark against one compiler binary.
fn run_lane(
    lane: HttpExecutionLane,
    compiler: &Path,
    workload: HttpPerformanceWorkload,
) -> Result<HttpPerformanceReport, String> {
    if !compiler.is_file() {
        return Err(format!(
            "compiler binary `{}` does not exist",
            compiler.display()
        ));
    }
    let execution = http_benchmark_support::BenchmarkExecutionMetadata::capture(
        compiler,
        workload.readiness_reactors,
    )?;
    let workspace = create_workspace()?;
    let web_root = write_package(&workspace, "generation-one", workload.payload_bytes)?;
    let port = reserve_port()?;
    let mut server = ServerGuard::new(spawn_server(
        compiler,
        &web_root,
        port,
        workload.readiness_reactors,
    )?);
    wait_for_generation(port, "generation-one", workload.payload_bytes)?;
    let protocol_validation =
        http_benchmark_support::validate_with_curl(port, workload.payload_bytes, "generation-one")?;
    let protocol_scenarios = http_benchmark_support::protocol_scenarios(port)?;
    let idle_memory = process_memory_snapshot(server.id(), "idle_after_readiness");

    // Readiness proves correctness; a separate sustained warm-up stabilizes
    // all runtime layers and is deliberately excluded from every timing.
    measure_requests(port, 1, workload.warmup_requests, workload.payload_bytes)?;
    // Warm every readiness owner and the host frequency state before any
    // sequential or pressure policy sample is selected.
    let pressure_warmup_per_worker = workload
        .warmup_requests
        .div_ceil(workload.concurrency)
        .max(1);
    measure_requests(
        port,
        workload.concurrency,
        pressure_warmup_per_worker,
        workload.payload_bytes,
    )?;

    let mut memory_snapshots = Vec::new();
    if let Some(snapshot) = idle_memory {
        memory_snapshots.push(snapshot);
    }
    if let Some(snapshot) = process_memory_snapshot(server.id(), "after_warmup") {
        memory_snapshots.push(snapshot);
    }
    let memory_before = resident_bytes(server.id());
    let efficiency_before = http_client::process_efficiency_snapshot(server.id());
    let (sequential, sequential_rounds) = measure_request_rounds(
        port,
        1,
        workload.sequential_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    // Sequential traffic intentionally exercises one sticky owner. Re-warm
    // every readiness owner immediately before pressure so cold remote shards
    // from the prior phase cannot decide the pressure median.
    measure_requests(
        port,
        workload.concurrency,
        pressure_warmup_per_worker,
        workload.payload_bytes,
    )?;
    let (pressure_timing, pressure_rounds) = measure_request_rounds(
        port,
        workload.concurrency,
        workload.requests_per_worker,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    let pressure_attempted = pressure_timing.sample_count;
    let memory_after_pressure = resident_bytes(server.id());
    if let Some(snapshot) = process_memory_snapshot(server.id(), "after_pressure") {
        memory_snapshots.push(snapshot);
    }
    let (longevity_timing, longevity_rounds) = measure_request_rounds(
        port,
        1,
        workload.longevity_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    let memory_after_longevity = resident_bytes(server.id());
    if let Some(snapshot) = process_memory_snapshot(server.id(), "after_longevity") {
        memory_snapshots.push(snapshot);
    }
    let efficiency_after = http_client::process_efficiency_snapshot(server.id());
    let additional_workloads = additional_workloads::measure(port, &workload)?;
    let maintained_workloads = http_benchmark_support::run_maintained_workload_matrix(
        port,
        workload.readiness_reactors,
        workload.concurrency,
        workload.payload_bytes,
        "generation-one",
    )?;
    let open_loop = http_benchmark_support::run_open_loop_saturation(
        port,
        workload.concurrency,
        workload.payload_bytes,
        "generation-one",
        &maintained_workloads,
    )?;
    let lifecycle = http_benchmark_support::lifecycle_scenarios(
        port,
        workload.payload_bytes,
        "generation-one",
        &maintained_workloads,
    )?;
    let external_load =
        http_benchmark_support::run_wrk_probe(port, workload.concurrency, workload.payload_bytes)?;
    let hardware_counters = http_benchmark_support::run_hardware_counter_probe(
        server.id(),
        port,
        workload.concurrency,
        workload.payload_bytes,
    )?;
    let soak = benchmark_evidence::measure_soak(port, server.id(), &workload)?;
    let memory_after_soak = resident_bytes(server.id());

    let generation_start = Instant::now();
    write_handler_source(&workspace, "generation-two")?;
    wait_for_generation(port, "generation-two", workload.payload_bytes)?;
    let memory_after_reload = resident_bytes(server.id());
    if let Some(snapshot) = process_memory_snapshot(server.id(), "after_reload") {
        memory_snapshots.push(snapshot);
    }
    let generation_latency = generation_start.elapsed().as_nanos();
    let peak = [
        memory_before,
        memory_after_pressure,
        memory_after_longevity,
        memory_after_soak,
        memory_after_reload,
    ]
    .into_iter()
    .flatten()
    .max();
    validate_run_limits(
        lane,
        peak,
        [&sequential_rounds, &pressure_rounds, &longevity_rounds],
    )?;
    let completed_requests = sequential_rounds
        .iter()
        .chain(&pressure_rounds)
        .chain(&longevity_rounds)
        .map(|timing| timing.sample_count)
        .sum::<usize>();
    let efficiency_requests = completed_requests.saturating_add(
        workload
            .concurrency
            .saturating_mul(pressure_warmup_per_worker),
    );
    let efficiency = efficiency_before
        .zip(efficiency_after)
        .map(|(before, after)| {
            http_client::efficiency_evidence(before, after, efficiency_requests)
        });
    let memory_attribution = http_client::memory_attribution(&memory_snapshots, completed_requests);
    server.stop();
    let _ = fs::remove_dir_all(&workspace);

    Ok(HttpPerformanceReport {
        schema: REPORT_SCHEMA.to_string(),
        status: "completed".to_string(),
        lane,
        timestamp_unix_seconds: unix_timestamp_seconds(),
        compiler_binary_sha256: sha256_file(compiler)?,
        hardware: HardwareFingerprint::current(),
        workload: workload.clone(),
        measurement: HttpMeasurementEvidence {
            aggregation: "median-throughput-round".to_string(),
            sequential_rounds,
            pressure_rounds,
            longevity_rounds,
        },
        sequential,
        allocation: HttpAllocationEvidence {
            measurement: "server_process_resident_set_bytes".to_string(),
            before_bytes: memory_before,
            after_pressure_bytes: memory_after_pressure,
            after_longevity_bytes: memory_after_longevity,
            peak_observed_bytes: peak,
            snapshots: memory_snapshots,
        },
        pressure: HttpPressureEvidence {
            concurrency: workload.concurrency,
            attempted_requests: pressure_attempted,
            completed_requests: pressure_attempted,
            failed_requests: 0,
            timing: pressure_timing,
        },
        longevity: HttpLongevityEvidence {
            attempted_requests: longevity_timing.sample_count,
            completed_requests: longevity_timing.sample_count,
            timing: longevity_timing,
        },
        generation_overlap: HttpGenerationEvidence {
            generations_observed: vec!["generation-one".to_string(), "generation-two".to_string()],
            reload_latency_ns: generation_latency,
            first_generation_body_verified: true,
            second_generation_body_verified: true,
            in_flight_lifetime_gate: "make tvm-aot-http-generation-lifetime-check".to_string(),
        },
        additional_workloads,
        benchmark_evidence: Some(benchmark_evidence::HttpExtendedBenchmarkEvidence {
            execution,
            integrity: benchmark_evidence::HttpIntegrityEvidence {
                attempted_requests: completed_requests,
                completed_requests,
                failed_requests: 0,
                response_body_verified: true,
            },
            protocol_validation,
            protocol_scenarios,
            external_load,
            maintained_workloads,
            open_loop,
            lifecycle,
            memory_attribution,
            hardware_counters,
            efficiency,
            soak,
        }),
    })
}

/// Applies opt-in absolute memory and repeatability gates to fresh evidence.
fn validate_run_limits(
    lane: HttpExecutionLane,
    peak_rss: Option<u64>,
    round_sets: [&[HttpTiming]; 3],
) -> Result<(), String> {
    if lane == HttpExecutionLane::NativeAot {
        if let Some(limit) = env::var("TERLAN_BENCH_HTTP_AOT_MAX_RSS_BYTES")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
        {
            let peak = peak_rss.ok_or_else(|| {
                "HTTP native AOT memory gate requires a resident-memory sample".to_string()
            })?;
            if peak > limit {
                return Err(format!(
                    "error[http_aot.memory_regression]: peak RSS {peak} exceeds {limit}"
                ));
            }
        }
    }
    if let Some(limit) = env::var("TERLAN_BENCH_HTTP_MAX_ROUND_SPREAD_PERCENT")
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
    {
        for (name, rounds) in ["sequential", "pressure", "longevity"]
            .into_iter()
            .zip(round_sets)
        {
            let minimum = rounds
                .iter()
                .map(|round| round.throughput_requests_per_second)
                .min()
                .unwrap_or(0);
            let maximum = rounds
                .iter()
                .map(|round| round.throughput_requests_per_second)
                .max()
                .unwrap_or(0);
            let spread = maximum
                .saturating_sub(minimum)
                .saturating_mul(100)
                .checked_div(minimum.max(1))
                .unwrap_or(u128::MAX);
            if spread > limit {
                return Err(format!(
                    "error[http_aot.unstable]: {name} throughput spread {spread}% exceeds {limit}%"
                ));
            }
        }
    }
    Ok(())
}

/// Records protocol and payload shapes that can move independently.
/// Reads both lane reports, validates comparability, and writes one result.
fn compare_from_env() -> Result<PathBuf, String> {
    let checked_path = env::var_os("TERLAN_BENCH_HTTP_CHECKED_COREIR_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CHECKED_COREIR_OUTPUT));
    let native_path = env::var_os("TERLAN_BENCH_HTTP_NATIVE_AOT_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_NATIVE_OUTPUT));
    let output = env::var_os("TERLAN_BENCH_HTTP_AOT_COMPARISON_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_COMPARISON_OUTPUT));
    let policy_path = env::var_os("TERLAN_BENCH_HTTP_AOT_POLICY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_POLICY));
    let checked_bytes = read_file(&checked_path)?;
    let native_bytes = read_file(&native_path)?;
    let policy_bytes = read_file(&policy_path)?;
    let checked = parse_report(&checked_path, &checked_bytes)?;
    let native = parse_report(&native_path, &native_bytes)?;
    let performance_policy = policy::parse_policy(&policy_path, &policy_bytes)?;
    let comparison = compare_reports_with_policy(
        checked,
        native,
        sha256(&checked_bytes),
        sha256(&native_bytes),
        performance_policy,
        sha256(&policy_bytes),
    )?;
    write_json(&output, &comparison)?;
    Ok(output)
}

/// Validates lane identity, workload equality, fingerprints, and evidence.
fn compare_reports(
    checked: HttpPerformanceReport,
    native: HttpPerformanceReport,
    checked_sha256: String,
    native_sha256: String,
) -> Result<HttpPerformanceComparison, String> {
    compare_reports_with_policy(
        checked,
        native,
        checked_sha256,
        native_sha256,
        policy::canonical_policy()?,
        "embedded-canonical-policy".to_string(),
    )
}

/// Validates and compares two reports under one explicit performance budget.
fn compare_reports_with_policy(
    checked: HttpPerformanceReport,
    native: HttpPerformanceReport,
    checked_sha256: String,
    native_sha256: String,
    performance_policy: policy::HttpPerformancePolicy,
    performance_policy_sha256: String,
) -> Result<HttpPerformanceComparison, String> {
    validate_report(&checked, HttpExecutionLane::CheckedCoreir)?;
    validate_report(&native, HttpExecutionLane::NativeAot)?;
    if checked.hardware.sha256 != native.hardware.sha256 {
        return Err(format!(
            "HTTP benchmark hardware fingerprints differ: checked-CoreIR={} native-AOT={}",
            checked.hardware.sha256, native.hardware.sha256
        ));
    }
    if !comparable_workloads(&checked, &native) {
        return Err("HTTP benchmark workloads differ between runtime lanes".to_string());
    }
    policy::validate_performance(&checked, &native, &performance_policy)?;
    let deltas = deltas::comparison_deltas(&checked, &native);
    Ok(HttpPerformanceComparison {
        schema: COMPARISON_SCHEMA,
        status: "completed",
        hardware_fingerprint_sha256: checked.hardware.sha256.clone(),
        workload: native.workload.clone(),
        checked_coreir_report_sha256: checked_sha256,
        native_aot_report_sha256: native_sha256,
        performance_policy_sha256,
        performance_policy,
        checked_coreir: checked,
        native_aot: native,
        deltas,
    })
}

/// Rejects incomplete reports before any comparison is published.
fn validate_report(
    report: &HttpPerformanceReport,
    expected_lane: HttpExecutionLane,
) -> Result<(), String> {
    let legacy_reference = report.schema == LEGACY_CHECKED_COREIR_SCHEMA
        && expected_lane == HttpExecutionLane::CheckedCoreir;
    if (!legacy_reference && report.schema != REPORT_SCHEMA) || report.status != "completed" {
        return Err("HTTP benchmark report is not a completed v2 report".to_string());
    }
    if report.lane != expected_lane {
        return Err(format!("unexpected HTTP benchmark lane: {:?}", report.lane));
    }
    for (name, timing) in [
        ("sequential", &report.sequential),
        ("pressure", &report.pressure.timing),
        ("longevity", &report.longevity.timing),
    ] {
        if timing.sample_count == 0
            || timing.throughput_requests_per_second == 0
            || timing.min_ns > timing.p50_ns
            || timing.p50_ns > timing.p95_ns
            || timing.p95_ns > timing.p99_ns
            || timing.p99_ns > timing.max_ns
        {
            return Err(format!(
                "HTTP benchmark {name} timing is incomplete or unordered"
            ));
        }
    }
    if report.measurement.aggregation != "median-throughput-round"
        || report.workload.measurement_rounds < 1
        || report.measurement.sequential_rounds.len() != report.workload.measurement_rounds
        || report.measurement.pressure_rounds.len() != report.workload.measurement_rounds
        || report.measurement.longevity_rounds.len() != report.workload.measurement_rounds
        || median_throughput_round(&report.measurement.sequential_rounds)?
            .throughput_requests_per_second
            != report.sequential.throughput_requests_per_second
        || median_throughput_round(&report.measurement.pressure_rounds)?
            .throughput_requests_per_second
            != report.pressure.timing.throughput_requests_per_second
        || median_throughput_round(&report.measurement.longevity_rounds)?
            .throughput_requests_per_second
            != report.longevity.timing.throughput_requests_per_second
    {
        return Err("HTTP benchmark repeated-round evidence is incomplete".to_string());
    }
    for workload in &report.additional_workloads {
        if workload.rounds.len() != report.workload.measurement_rounds
            || median_throughput_round(&workload.rounds)?.throughput_requests_per_second
                != workload.timing.throughput_requests_per_second
        {
            return Err(format!(
                "HTTP benchmark `{}` repeated-round evidence is incomplete",
                workload.name
            ));
        }
    }
    if report.pressure.completed_requests != report.pressure.attempted_requests
        || report.pressure.failed_requests != 0
        || report.longevity.completed_requests != report.longevity.attempted_requests
        || report.allocation.peak_observed_bytes.is_none()
        || report.generation_overlap.generations_observed.len() < 2
        || !report.generation_overlap.first_generation_body_verified
        || !report.generation_overlap.second_generation_body_verified
    {
        return Err("HTTP benchmark lifecycle evidence is incomplete".to_string());
    }
    if expected_lane == HttpExecutionLane::NativeAot {
        let evidence = report.benchmark_evidence.as_ref().ok_or_else(|| {
            "native AOT HTTP benchmark lacks reproducibility evidence".to_string()
        })?;
        if evidence.integrity.attempted_requests != evidence.integrity.completed_requests
            || evidence.integrity.failed_requests != 0
            || !evidence.integrity.response_body_verified
            || evidence.protocol_validation.status != "validated"
            || evidence.execution.server_binary_sha256.len() != 64
            || !evidence.protocol_scenarios.iter().any(|scenario| {
                scenario.name == "error-response-404" && scenario.status == "validated"
            })
        {
            return Err("native AOT HTTP benchmark extended evidence is incomplete".to_string());
        }
    }
    Ok(())
}

fn comparable_workloads(checked: &HttpPerformanceReport, native: &HttpPerformanceReport) -> bool {
    if checked.schema != LEGACY_CHECKED_COREIR_SCHEMA {
        return checked.workload == native.workload;
    }
    checked.workload.sequential_requests == native.workload.sequential_requests
        && checked.workload.concurrency == native.workload.concurrency
        && checked.workload.requests_per_worker == native.workload.requests_per_worker
        && checked.workload.longevity_requests == native.workload.longevity_requests
        && checked.workload.payload_bytes == native.workload.payload_bytes
}

#[cfg(test)]
#[path = "http_aot_performance_test.rs"]
mod tests;
