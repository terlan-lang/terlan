//! Comparable checked-CoreIR and native-AOT HTTP benchmark harness.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::hardware::{sha256, HardwareFingerprint};
use serde::{Deserialize, Serialize};

#[path = "http_aot_performance/deltas.rs"]
mod deltas;

#[path = "http_aot_performance/policy.rs"]
mod policy;

/// Benchmark command that records one executable HTTP lane.
pub(crate) const COMMAND: &str = "http-aot-performance";
/// Benchmark command that compares two previously recorded lanes.
pub(crate) const COMPARE_COMMAND: &str = "http-aot-performance-compare";
/// Pure report-contract self-test command used when socket benchmarks are unavailable.
pub(crate) const SELF_TEST_COMMAND: &str = "http-aot-performance-self-test";

const REPORT_SCHEMA: &str = "terlan-http-aot-performance-v1";
const COMPARISON_SCHEMA: &str = "terlan-http-aot-performance-comparison-v1";
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
    sequential_requests: usize,
    concurrency: usize,
    requests_per_worker: usize,
    longevity_requests: usize,
    payload_bytes: usize,
}

impl HttpPerformanceWorkload {
    /// Reads benchmark workload controls while retaining non-zero defaults.
    fn from_env() -> Self {
        Self {
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

impl HttpTiming {
    /// Summarizes non-empty request durations against one wall-clock interval.
    fn from_durations(durations: &[Duration], wall: Duration) -> Result<Self, String> {
        if durations.is_empty() {
            return Err("HTTP benchmark timing requires at least one sample".to_string());
        }
        let mut values = durations.iter().map(Duration::as_nanos).collect::<Vec<_>>();
        values.sort_unstable();
        let total = values.iter().sum::<u128>();
        let wall_ns = wall.as_nanos().max(1);
        Ok(Self {
            sample_count: values.len(),
            total_wall_ns: wall_ns,
            throughput_requests_per_second: values.len() as u128 * 1_000_000_000 / wall_ns,
            min_ns: values[0],
            mean_ns: total / values.len() as u128,
            p50_ns: percentile(&values, 50),
            p95_ns: percentile(&values, 95),
            p99_ns: percentile(&values, 99),
            max_ns: values[values.len() - 1],
        })
    }
}

/// Process resident-memory observations around the measured workload.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpAllocationEvidence {
    measurement: String,
    before_bytes: Option<u64>,
    after_pressure_bytes: Option<u64>,
    after_longevity_bytes: Option<u64>,
    peak_observed_bytes: Option<u64>,
}

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
    sequential: HttpTiming,
    allocation: HttpAllocationEvidence,
    pressure: HttpPressureEvidence,
    longevity: HttpLongevityEvidence,
    generation_overlap: HttpGenerationEvidence,
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
            sequential_requests: 2,
            concurrency: 2,
            requests_per_worker: 1,
            longevity_requests: 2,
            payload_bytes: 8,
        },
        sequential: timing.clone(),
        allocation: HttpAllocationEvidence {
            measurement: "server_process_resident_set_bytes".to_string(),
            before_bytes: Some(1),
            after_pressure_bytes: Some(2),
            after_longevity_bytes: Some(2),
            peak_observed_bytes: Some(2),
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
    let workspace = create_workspace()?;
    let web_root = write_package(&workspace, "generation-one", workload.payload_bytes)?;
    let port = reserve_port()?;
    let mut server = ServerGuard::new(spawn_server(compiler, &web_root, port)?);
    wait_for_generation(port, "generation-one", workload.payload_bytes)?;

    let memory_before = resident_bytes(server.id());
    let sequential = measure_requests(
        port,
        1,
        workload.sequential_requests,
        workload.payload_bytes,
    )?;
    let pressure_timing = measure_requests(
        port,
        workload.concurrency,
        workload.requests_per_worker,
        workload.payload_bytes,
    )?;
    let pressure_attempted = workload.concurrency * workload.requests_per_worker;
    let memory_after_pressure = resident_bytes(server.id());
    let longevity_timing =
        measure_requests(port, 1, workload.longevity_requests, workload.payload_bytes)?;
    let memory_after_longevity = resident_bytes(server.id());

    let generation_start = Instant::now();
    write_handler_source(&workspace, "generation-two")?;
    wait_for_generation(port, "generation-two", workload.payload_bytes)?;
    let generation_latency = generation_start.elapsed().as_nanos();
    let peak = [memory_before, memory_after_pressure, memory_after_longevity]
        .into_iter()
        .flatten()
        .max();
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
        sequential,
        allocation: HttpAllocationEvidence {
            measurement: "server_process_resident_set_bytes".to_string(),
            before_bytes: memory_before,
            after_pressure_bytes: memory_after_pressure,
            after_longevity_bytes: memory_after_longevity,
            peak_observed_bytes: peak,
        },
        pressure: HttpPressureEvidence {
            concurrency: workload.concurrency,
            attempted_requests: pressure_attempted,
            completed_requests: pressure_attempted,
            failed_requests: 0,
            timing: pressure_timing,
        },
        longevity: HttpLongevityEvidence {
            attempted_requests: workload.longevity_requests,
            completed_requests: workload.longevity_requests,
            timing: longevity_timing,
        },
        generation_overlap: HttpGenerationEvidence {
            generations_observed: vec!["generation-one".to_string(), "generation-two".to_string()],
            reload_latency_ns: generation_latency,
            first_generation_body_verified: true,
            second_generation_body_verified: true,
            in_flight_lifetime_gate: "make tvm-aot-http-generation-lifetime-check".to_string(),
        },
    })
}

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
    if checked.workload != native.workload {
        return Err("HTTP benchmark workloads differ between runtime lanes".to_string());
    }
    policy::validate_performance(&checked, &native, &performance_policy)?;
    let deltas = deltas::comparison_deltas(&checked, &native);
    Ok(HttpPerformanceComparison {
        schema: COMPARISON_SCHEMA,
        status: "completed",
        hardware_fingerprint_sha256: checked.hardware.sha256.clone(),
        workload: checked.workload.clone(),
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
    if report.schema != REPORT_SCHEMA || report.status != "completed" {
        return Err("HTTP benchmark report is not a completed v1 report".to_string());
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
    Ok(())
}

/// Measures individual request latency and aggregate wall-clock throughput.
fn measure_requests(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let started = Instant::now();
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        workers.push(thread::spawn(move || {
            let mut durations = Vec::with_capacity(requests_per_worker);
            for _ in 0..requests_per_worker {
                let request_started = Instant::now();
                let body = request(port, payload_bytes)?;
                if !body.starts_with("generation-") {
                    return Err(format!("unexpected benchmark response body `{body}`"));
                }
                durations.push(request_started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    let mut durations = Vec::with_capacity(concurrency * requests_per_worker);
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP benchmark worker panicked".to_string())??,
        );
    }
    HttpTiming::from_durations(&durations, started.elapsed())
}

/// Sends one complete loopback request and returns its validated response body.
fn request(port: u16, payload_bytes: usize) -> Result<String, String> {
    let payload = "x".repeat(payload_bytes);
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("HTTP benchmark connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP benchmark read timeout setup failed: {error}"))?;
    write!(
        stream,
        "POST /api/bench HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    )
    .map_err(|error| format!("HTTP benchmark request write failed: {error}"))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("HTTP benchmark response read failed: {error}"))?;
    let response = String::from_utf8(bytes)
        .map_err(|error| format!("HTTP benchmark response was not UTF-8: {error}"))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP benchmark response lacked a header terminator".to_string())?;
    if !head.lines().next().unwrap_or_default().contains(" 200 ") {
        return Err(format!("HTTP benchmark returned non-200 response `{head}`"));
    }
    Ok(body.to_string())
}

/// Waits for one named source generation to become visible through HTTP.
fn wait_for_generation(port: u16, generation: &str, payload_bytes: usize) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    while Instant::now() < deadline {
        match request(port, payload_bytes) {
            Ok(body) if body == format!("{generation}:{}", "x".repeat(payload_bytes)) => {
                return Ok(())
            }
            Ok(body) => last = body,
            Err(error) => last = error,
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "HTTP benchmark generation `{generation}` did not become ready; last result `{last}`"
    ))
}

/// Creates an isolated benchmark package and returns its web root.
fn write_package(
    workspace: &Path,
    generation: &str,
    _payload_bytes: usize,
) -> Result<PathBuf, String> {
    let web_root = workspace.join("_build/web");
    fs::create_dir_all(web_root.join("assets/js/modules"))
        .map_err(|error| format!("failed to create HTTP benchmark web root: {error}"))?;
    fs::create_dir_all(workspace.join("src/app"))
        .map_err(|error| format!("failed to create HTTP benchmark source root: {error}"))?;
    fs::write(
        workspace.join("terlan.toml"),
        "[package]\nname = \"http_aot_performance\"\nversion = \"0.0.7\"\n",
    )
    .map_err(|error| format!("failed to write HTTP benchmark manifest: {error}"))?;
    fs::write(web_root.join("index.html"), "<!doctype html>\n")
        .map_err(|error| format!("failed to write HTTP benchmark index: {error}"))?;
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const benchmark = true;\n",
    )
    .map_err(|error| format!("failed to write HTTP benchmark asset: {error}"))?;
    write_handler_source(workspace, generation)?;
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [{
    "method": "POST",
    "route": "/api/bench",
    "module": "app.Api",
    "function": "handle",
    "arity": 1,
    "source": {"path": "src/app/Api.terl", "line": 7, "column": 5}
  }],
  "assets": [{
    "module": "app",
    "kind": "javascript-module",
    "source_relative_path": "modules/app.js",
    "web_relative_path": "assets/js/modules/app.js",
    "fingerprint": 1
  }]
}
"#,
    )
    .map_err(|error| format!("failed to write HTTP benchmark web manifest: {error}"))?;
    Ok(web_root)
}

/// Writes the source handler for one distinguishable generation.
fn write_handler_source(workspace: &Path, generation: &str) -> Result<(), String> {
    fs::write(
        workspace.join("src/app/Api.terl"),
        format!(
            "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{{Request}}.\nimport type std.http.Response.{{Response}}.\n\npub handle(request: Request): Response ->\n    Response.text(\"{generation}:\" + request.body_text()).\n"
        ),
    )
    .map_err(|error| format!("failed to write HTTP benchmark handler source: {error}"))
}

/// Spawns `terlc serve` with fast source generation polling.
fn spawn_server(compiler: &Path, web_root: &Path, port: u16) -> Result<Child, String> {
    let web_root = web_root.to_string_lossy().to_string();
    let port = port.to_string();
    Command::new(compiler)
        .args([
            "serve",
            &web_root,
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "--poll-ms",
            "25",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to start HTTP benchmark server: {error}"))
}

/// Drop guard that always terminates and reaps a benchmark server.
struct ServerGuard {
    child: Child,
}

impl ServerGuard {
    /// Wraps a newly spawned benchmark server.
    fn new(child: Child) -> Self {
        Self { child }
    }

    /// Returns the operating-system process identifier.
    fn id(&self) -> u32 {
        self.child.id()
    }

    /// Terminates the server before normal report serialization.
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServerGuard {
    /// Ensures failed benchmark paths do not retain a serving process.
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reserves a currently unused loopback port.
fn reserve_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to reserve HTTP benchmark port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("failed to inspect HTTP benchmark port: {error}"))
}

/// Creates a unique temporary benchmark workspace.
fn create_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-http-aot-performance-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create HTTP benchmark workspace: {error}"))?;
    Ok(path)
}

/// Reads resident-set bytes for a child process through procfs or `ps`.
fn resident_bytes(pid: u32) -> Option<u64> {
    let procfs_kilobytes = fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        });
    let kilobytes = procfs_kilobytes.or_else(|| {
        Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    })?;
    kilobytes.checked_mul(1024)
}

/// Computes the digest of one compiler or report file.
fn sha256_file(path: &Path) -> Result<String, String> {
    read_file(path).map(|bytes| sha256(&bytes))
}

/// Reads one required file with path-aware diagnostics.
fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("failed to read `{}`: {error}", path.display()))
}

/// Parses one typed lane report.
fn parse_report(path: &Path, bytes: &[u8]) -> Result<HttpPerformanceReport, String> {
    serde_json::from_slice(bytes)
        .map_err(|error| format!("failed to parse `{}`: {error}", path.display()))
}

/// Writes one pretty JSON report after creating its parent directory.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create `{}`: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to serialize HTTP benchmark report: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

/// Reads a positive integer environment option or returns its default.
fn read_positive_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Computes a nearest-rank percentile from sorted values.
fn percentile(sorted: &[u128], requested: usize) -> u128 {
    let index = ((sorted.len() - 1) * requested).div_ceil(100);
    sorted[index]
}

/// Returns the current Unix timestamp in seconds.
fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Returns the current Unix timestamp in nanoseconds for unique paths.
fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "http_aot_performance_test.rs"]
mod tests;
