#![forbid(unsafe_code)]

//! Framework-neutral HTTP client benchmark for the Axum/Tokio control lane.

use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

#[path = "hardware.rs"]
mod hardware;
#[path = "http_benchmark_support.rs"]
mod http_benchmark_support;
#[path = "http_client.rs"]
mod http_client;

#[derive(Clone, Serialize)]
struct Workload {
    warmup_requests: usize,
    measurement_rounds: usize,
    readiness_reactors: usize,
    sequential_requests: usize,
    concurrency: usize,
    requests_per_worker: usize,
    longevity_requests: usize,
    payload_bytes: usize,
    measurement_duration_ms: u64,
    soak_seconds: u64,
}

impl Workload {
    fn from_env() -> Self {
        let readiness_reactors = positive_env(
            "TERLAN_BENCH_HTTP_AOT_REACTORS",
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        )
        .min(32);
        Self {
            warmup_requests: positive_env(
                "TERLAN_BENCH_HTTP_AOT_WARMUP",
                readiness_reactors.saturating_mul(800),
            ),
            measurement_rounds: positive_env("TERLAN_BENCH_HTTP_AOT_ROUNDS", 5),
            readiness_reactors,
            sequential_requests: positive_env("TERLAN_BENCH_HTTP_AOT_ITERATIONS", 500),
            concurrency: positive_env("TERLAN_BENCH_HTTP_AOT_CONCURRENCY", 8),
            requests_per_worker: positive_env("TERLAN_BENCH_HTTP_AOT_REQUESTS_PER_WORKER", 100),
            longevity_requests: positive_env("TERLAN_BENCH_HTTP_AOT_LONGEVITY", 1_000),
            payload_bytes: positive_env("TERLAN_BENCH_HTTP_AOT_PAYLOAD_BYTES", 512),
            measurement_duration_ms: non_negative_env("TERLAN_BENCH_HTTP_DURATION_MS", 0),
            soak_seconds: non_negative_env("TERLAN_BENCH_HTTP_SOAK_SECONDS", 0),
        }
    }
}

#[derive(Clone, Serialize)]
struct Timing {
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

#[derive(Serialize)]
struct NamedWorkload {
    name: String,
    connection_mode: String,
    concurrency: usize,
    requests: usize,
    payload_bytes: usize,
    timing: Timing,
    rounds: Vec<Timing>,
}

#[derive(Serialize)]
struct IntegrityEvidence {
    attempted_requests: usize,
    completed_requests: usize,
    failed_requests: usize,
    response_body_verified: bool,
}

#[derive(Serialize)]
struct SoakEvidence {
    duration_seconds: u64,
    timing: Timing,
    memory_before: Option<http_client::ProcessMemorySnapshot>,
    memory_after: Option<http_client::ProcessMemorySnapshot>,
    resident_growth_bytes: Option<i64>,
    maximum_growth_bytes: u64,
    status: &'static str,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    implementation: String,
    hardware: hardware::HardwareFingerprint,
    execution: http_benchmark_support::BenchmarkExecutionMetadata,
    workload: Workload,
    sequential_rounds: Vec<Timing>,
    pressure_rounds: Vec<Timing>,
    longevity_rounds: Vec<Timing>,
    sequential: Timing,
    pressure: Timing,
    longevity: Timing,
    resident_memory_before_bytes: Option<u64>,
    resident_memory_peak_bytes: Option<u64>,
    memory_snapshots: Vec<http_client::ProcessMemorySnapshot>,
    memory_attribution: http_client::MemoryAttributionEvidence,
    additional_workloads: Vec<NamedWorkload>,
    maintained_workloads: Vec<http_benchmark_support::MaintainedWorkloadEvidence>,
    open_loop: http_benchmark_support::OpenLoopEvidence,
    lifecycle: http_benchmark_support::LifecycleEvidence,
    integrity: IntegrityEvidence,
    protocol_validation: http_benchmark_support::ProtocolValidationEvidence,
    protocol_scenarios: Vec<http_benchmark_support::ProtocolScenarioEvidence>,
    external_load: Option<http_benchmark_support::ExternalLoadEvidence>,
    hardware_counters: http_benchmark_support::HardwareCounterEvidence,
    efficiency: Option<http_client::ProcessEfficiencyEvidence>,
    soak: Option<SoakEvidence>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error[http-framework-benchmark]: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let binary = env::var_os("TERLAN_BENCH_HTTP_AXUM_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/release/terlan-axum-baseline"));
    if !binary.is_file() {
        return Err(format!("Axum server `{}` does not exist", binary.display()));
    }
    let output = env::var_os("TERLAN_BENCH_HTTP_AXUM_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/http-axum-performance.json"));
    let workload = Workload::from_env();
    let execution = http_benchmark_support::BenchmarkExecutionMetadata::capture(
        &binary,
        workload.readiness_reactors,
    )?;
    let port = reserve_port()?;
    let mut server = ServerGuard::spawn(&binary, port)?;
    wait_ready(port, workload.payload_bytes)?;
    let protocol_validation =
        http_benchmark_support::validate_with_curl(port, workload.payload_bytes, "generation-one")?;
    let protocol_scenarios = http_benchmark_support::protocol_scenarios(port)?;
    let idle_memory = http_client::process_memory_snapshot(server.id(), "idle_after_readiness");

    measure(port, 1, workload.warmup_requests, workload.payload_bytes)?;
    measure(
        port,
        workload.concurrency,
        workload
            .warmup_requests
            .div_ceil(workload.concurrency)
            .max(1),
        workload.payload_bytes,
    )?;

    let mut memory_snapshots = Vec::new();
    if let Some(snapshot) = idle_memory {
        memory_snapshots.push(snapshot);
    }
    if let Some(snapshot) = http_client::process_memory_snapshot(server.id(), "after_warmup") {
        memory_snapshots.push(snapshot);
    }
    let memory_before = http_client::resident_bytes(server.id());
    let efficiency_before = http_client::process_efficiency_snapshot(server.id());
    let (sequential, sequential_rounds) = rounds(
        port,
        1,
        workload.sequential_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    // Isolate pressure from the preceding sticky sequential phase in exactly
    // the same way as the Terlan lane.
    measure(
        port,
        workload.concurrency,
        workload
            .warmup_requests
            .div_ceil(workload.concurrency)
            .max(1),
        workload.payload_bytes,
    )?;
    let (pressure, pressure_rounds) = rounds(
        port,
        workload.concurrency,
        workload.requests_per_worker,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    let memory_pressure = http_client::resident_bytes(server.id());
    if let Some(snapshot) = http_client::process_memory_snapshot(server.id(), "after_pressure") {
        memory_snapshots.push(snapshot);
    }
    let (longevity, longevity_rounds) = rounds(
        port,
        1,
        workload.longevity_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
        workload.measurement_duration_ms,
    )?;
    let memory_longevity = http_client::resident_bytes(server.id());
    if let Some(snapshot) = http_client::process_memory_snapshot(server.id(), "after_longevity") {
        memory_snapshots.push(snapshot);
    }
    let efficiency_after = http_client::process_efficiency_snapshot(server.id());
    let additional_workloads = additional_workloads(port, &workload)?;
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
    let soak = measure_soak(port, server.id(), &workload)?;
    if let Some(snapshot) = http_client::process_memory_snapshot(server.id(), "after_all_workloads")
    {
        memory_snapshots.push(snapshot);
    }
    server.stop();

    let completed_requests: usize = sequential_rounds
        .iter()
        .chain(&pressure_rounds)
        .chain(&longevity_rounds)
        .map(|timing| timing.sample_count)
        .sum();
    let efficiency_requests = completed_requests.saturating_add(
        workload.concurrency.saturating_mul(
            workload
                .warmup_requests
                .div_ceil(workload.concurrency)
                .max(1),
        ),
    );
    let efficiency = efficiency_before
        .zip(efficiency_after)
        .map(|(before, after)| {
            http_client::efficiency_evidence(before, after, efficiency_requests)
        });
    let memory_attribution = http_client::memory_attribution(&memory_snapshots, completed_requests);
    let report = Report {
        schema: "terlan-http-framework-performance-v2",
        status: "completed",
        implementation: env::var("TERLAN_BENCH_HTTP_IMPLEMENTATION")
            .unwrap_or_else(|_| "axum-0.8.9+tokio-1.52.3".to_string()),
        hardware: hardware::HardwareFingerprint::current(),
        execution,
        workload,
        sequential_rounds,
        pressure_rounds,
        longevity_rounds,
        sequential,
        pressure,
        longevity,
        resident_memory_before_bytes: memory_before,
        resident_memory_peak_bytes: [memory_before, memory_pressure, memory_longevity]
            .into_iter()
            .flatten()
            .max(),
        memory_snapshots,
        memory_attribution,
        additional_workloads,
        maintained_workloads,
        open_loop,
        lifecycle,
        integrity: IntegrityEvidence {
            attempted_requests: completed_requests,
            completed_requests,
            failed_requests: 0,
            response_body_verified: true,
        },
        protocol_validation,
        protocol_scenarios,
        external_load,
        hardware_counters,
        efficiency,
        soak,
    };
    validate_report(&report)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        &output,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!("[http-axum-performance] wrote {}", output.display());
    Ok(())
}

fn validate_report(report: &Report) -> Result<(), String> {
    let rounds = report.workload.measurement_rounds;
    if rounds == 0
        || report.sequential_rounds.len() != rounds
        || report.pressure_rounds.len() != rounds
        || report.longevity_rounds.len() != rounds
        || report
            .additional_workloads
            .iter()
            .any(|workload| workload.rounds.len() != rounds)
    {
        return Err("framework report has incomplete measurement rounds".to_string());
    }
    if report.integrity.attempted_requests != report.integrity.completed_requests
        || report.integrity.failed_requests != 0
        || !report.integrity.response_body_verified
        || report.protocol_validation.status != "validated"
        || ["http-1.1", "error-response-404"]
            .into_iter()
            .any(|required| {
                !report
                    .protocol_scenarios
                    .iter()
                    .any(|scenario| scenario.name == required && scenario.status == "validated")
            })
        || report.execution.server_binary_sha256.len() != 64
        || report.hardware.sha256.len() != 64
    {
        return Err("framework report failed integrity or provenance validation".to_string());
    }
    Ok(())
}

fn additional_workloads(port: u16, workload: &Workload) -> Result<Vec<NamedWorkload>, String> {
    let short_requests = workload.sequential_requests.clamp(64, 500);
    let concurrent_requests = workload.requests_per_worker.clamp(32, 200);
    let large_concurrency = workload.concurrency.min(4);
    let churn = arbitrary_rounds(workload.measurement_rounds, || {
        measure_aux_close(port, 1, short_requests, 0, workload.measurement_duration_ms)
    })?;
    let persistent = arbitrary_rounds(workload.measurement_rounds, || {
        measure_aux_keep_alive(
            port,
            workload.concurrency,
            concurrent_requests,
            workload.payload_bytes,
            workload.measurement_duration_ms,
        )
    })?;
    let large = arbitrary_rounds(workload.measurement_rounds, || {
        measure_aux_close(
            port,
            large_concurrency,
            32,
            64 * 1024,
            workload.measurement_duration_ms,
        )
    })?;
    let mut workloads = vec![
        NamedWorkload {
            name: "empty-connection-churn".to_string(),
            connection_mode: "close".to_string(),
            concurrency: 1,
            requests: short_requests,
            payload_bytes: 0,
            timing: churn.0,
            rounds: churn.1,
        },
        NamedWorkload {
            name: "persistent-small-body".to_string(),
            connection_mode: "keep-alive".to_string(),
            concurrency: workload.concurrency,
            requests: workload.concurrency.saturating_mul(concurrent_requests),
            payload_bytes: workload.payload_bytes,
            timing: persistent.0,
            rounds: persistent.1,
        },
        NamedWorkload {
            name: "large-body-64k".to_string(),
            connection_mode: "close".to_string(),
            concurrency: large_concurrency,
            requests: large_concurrency.saturating_mul(32),
            payload_bytes: 64 * 1024,
            timing: large.0,
            rounds: large.1,
        },
    ];
    if matrix_enabled() {
        for (name, concurrency, payload) in matrix_cases(workload) {
            let measured = arbitrary_rounds(workload.measurement_rounds, || {
                measure_aux_close(
                    port,
                    concurrency,
                    32,
                    payload,
                    workload.measurement_duration_ms,
                )
            })?;
            workloads.push(NamedWorkload {
                name,
                connection_mode: "close".to_string(),
                concurrency,
                requests: concurrency.saturating_mul(32),
                payload_bytes: payload,
                timing: measured.0,
                rounds: measured.1,
            });
        }
        let headers = arbitrary_rounds(workload.measurement_rounds, || {
            measure_aux_shaped(port, workload, 32, Duration::ZERO)
        })?;
        workloads.push(NamedWorkload {
            name: "matrix-headers-32".to_string(),
            connection_mode: "close".to_string(),
            concurrency: workload.concurrency,
            requests: workload.concurrency.saturating_mul(32),
            payload_bytes: workload.payload_bytes,
            timing: headers.0,
            rounds: headers.1,
        });
        let slow_reader = arbitrary_rounds(workload.measurement_rounds, || {
            measure_aux_shaped(port, workload, 0, Duration::from_millis(5))
        })?;
        workloads.push(NamedWorkload {
            name: "matrix-slow-reader-5ms".to_string(),
            connection_mode: "close-delayed-read".to_string(),
            concurrency: workload.concurrency,
            requests: workload.concurrency.saturating_mul(16),
            payload_bytes: workload.payload_bytes,
            timing: slow_reader.0,
            rounds: slow_reader.1,
        });
    }
    Ok(workloads)
}

fn measure_aux_close(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    duration_ms: u64,
) -> Result<Timing, String> {
    if duration_ms == 0 {
        measure(port, concurrency, requests, payload)
    } else {
        timing_from_measurement(http_client::measure_for_duration(
            port,
            concurrency,
            Duration::from_millis(duration_ms),
            payload,
            "generation-one",
        )?)
    }
}

fn measure_aux_keep_alive(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    duration_ms: u64,
) -> Result<Timing, String> {
    if duration_ms == 0 {
        measure_keep_alive(port, concurrency, requests, payload)
    } else {
        timing_from_measurement(http_client::measure_keep_alive_for_duration(
            port,
            concurrency,
            Duration::from_millis(duration_ms),
            payload,
            "generation-one",
        )?)
    }
}

fn measure_aux_shaped(
    port: u16,
    workload: &Workload,
    headers: usize,
    delay: Duration,
) -> Result<Timing, String> {
    let measured = if workload.measurement_duration_ms == 0 {
        http_client::measure_shaped(
            port,
            workload.concurrency,
            if delay.is_zero() { 32 } else { 16 },
            workload.payload_bytes,
            "generation-one",
            headers,
            delay,
        )?
    } else {
        http_client::measure_shaped_for_duration(
            port,
            workload.concurrency,
            Duration::from_millis(workload.measurement_duration_ms),
            workload.payload_bytes,
            "generation-one",
            headers,
            delay,
        )?
    };
    timing_from_measurement(measured)
}

fn arbitrary_rounds(
    count: usize,
    mut measure: impl FnMut() -> Result<Timing, String>,
) -> Result<(Timing, Vec<Timing>), String> {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(measure()?);
    }
    select_median(samples)
}

fn rounds(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    count: usize,
    duration_ms: u64,
) -> Result<(Timing, Vec<Timing>), String> {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(if duration_ms == 0 {
            measure(port, concurrency, requests, payload)?
        } else {
            timing_from_measurement(http_client::measure_for_duration(
                port,
                concurrency,
                Duration::from_millis(duration_ms),
                payload,
                "generation-one",
            )?)?
        });
    }
    select_median(samples)
}

fn measure_soak(port: u16, pid: u32, workload: &Workload) -> Result<Option<SoakEvidence>, String> {
    if workload.soak_seconds == 0 {
        return Ok(None);
    }
    let memory_before = http_client::process_memory_snapshot(pid, "before_soak");
    let timing = timing_from_measurement(http_client::measure_keep_alive_for_duration(
        port,
        workload.concurrency,
        Duration::from_secs(workload.soak_seconds),
        workload.payload_bytes,
        "generation-one",
    )?)?;
    let memory_after = http_client::process_memory_snapshot(pid, "after_soak");
    let maximum_growth_bytes =
        non_negative_env("TERLAN_BENCH_HTTP_SOAK_MAX_GROWTH_BYTES", 16 * 1024 * 1024);
    let resident_growth_bytes = memory_before
        .as_ref()
        .and_then(|before| before.rss_bytes)
        .zip(memory_after.as_ref().and_then(|after| after.rss_bytes))
        .map(|(before, after)| after as i64 - before as i64);
    let stable = resident_growth_bytes
        .is_none_or(|growth| growth <= i64::try_from(maximum_growth_bytes).unwrap_or(i64::MAX));
    if !stable {
        return Err(format!(
            "HTTP soak RSS growth {:?} exceeds {} bytes",
            resident_growth_bytes, maximum_growth_bytes
        ));
    }
    Ok(Some(SoakEvidence {
        duration_seconds: workload.soak_seconds,
        timing,
        memory_before,
        memory_after,
        resident_growth_bytes,
        maximum_growth_bytes,
        status: "stable",
    }))
}

fn matrix_enabled() -> bool {
    env::var("TERLAN_BENCH_HTTP_MATRIX")
        .map(|value| value != "0")
        .unwrap_or(true)
}

fn matrix_cases(workload: &Workload) -> Vec<(String, usize, usize)> {
    let cores = workload.readiness_reactors.max(1);
    vec![
        ("matrix-c1-empty".to_string(), 1, 0),
        ("matrix-cores-4k".to_string(), cores, 4 * 1024),
        (
            "matrix-oversubscribed-512".to_string(),
            cores.saturating_mul(2),
            512,
        ),
        ("matrix-c4-1m".to_string(), 4.min(cores), 1024 * 1024),
    ]
}

fn select_median(samples: Vec<Timing>) -> Result<(Timing, Vec<Timing>), String> {
    if samples.is_empty() {
        return Err("HTTP benchmark requires at least one measurement round".to_string());
    }
    let mut ranking = (0..samples.len()).collect::<Vec<_>>();
    ranking.sort_by_key(|index| samples[*index].throughput_requests_per_second);
    let selected = samples[ranking[ranking.len() / 2]].clone();
    Ok((selected, samples))
}

fn measure(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<Timing, String> {
    let measurement = http_client::measure(
        port,
        concurrency,
        requests_per_worker,
        payload_bytes,
        "generation-one",
    )?;
    let mut durations = measurement
        .durations
        .iter()
        .map(Duration::as_nanos)
        .collect::<Vec<_>>();
    durations.sort_unstable();
    let wall_ns = measurement.wall.as_nanos().max(1);
    let total = durations.iter().sum::<u128>();
    Ok(Timing {
        sample_count: durations.len(),
        total_wall_ns: wall_ns,
        throughput_requests_per_second: durations.len() as u128 * 1_000_000_000 / wall_ns,
        min_ns: durations[0],
        mean_ns: total / durations.len() as u128,
        p50_ns: percentile(&durations, 50),
        p95_ns: percentile(&durations, 95),
        p99_ns: percentile(&durations, 99),
        max_ns: durations[durations.len() - 1],
    })
}

fn measure_keep_alive(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<Timing, String> {
    let measurement = http_client::measure_keep_alive(
        port,
        concurrency,
        requests_per_worker,
        payload_bytes,
        "generation-one",
    )?;
    timing_from_measurement(measurement)
}

fn timing_from_measurement(
    measurement: http_client::HttpClientMeasurement,
) -> Result<Timing, String> {
    if measurement.durations.is_empty() {
        return Err("HTTP benchmark produced no latency samples".to_string());
    }
    let mut durations = measurement
        .durations
        .iter()
        .map(Duration::as_nanos)
        .collect::<Vec<_>>();
    durations.sort_unstable();
    let wall_ns = measurement.wall.as_nanos().max(1);
    let total = durations.iter().sum::<u128>();
    Ok(Timing {
        sample_count: durations.len(),
        total_wall_ns: wall_ns,
        throughput_requests_per_second: durations.len() as u128 * 1_000_000_000 / wall_ns,
        min_ns: durations[0],
        mean_ns: total / durations.len() as u128,
        p50_ns: percentile(&durations, 50),
        p95_ns: percentile(&durations, 95),
        p99_ns: percentile(&durations, 99),
        max_ns: durations[durations.len() - 1],
    })
}

fn wait_ready(port: u16, payload: usize) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if http_client::request(port, payload, "generation-one").is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err("Axum server did not become ready".to_string())
}

fn reserve_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

fn percentile(values: &[u128], percentile: usize) -> u128 {
    values[(values.len() * percentile).div_ceil(100).saturating_sub(1)]
}

fn positive_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn non_negative_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

struct ServerGuard(Child);

impl ServerGuard {
    fn spawn(binary: &Path, port: u16) -> Result<Self, String> {
        let affinity = env::var("TERLAN_BENCH_HTTP_CPU_LIST")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let mut command = if let Some(affinity) = affinity {
            let mut command = Command::new("taskset");
            command.args(["--cpu-list", affinity.as_str()]);
            command.arg(binary);
            command
        } else {
            Command::new(binary)
        };
        command
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(Self)
            .map_err(|error| error.to_string())
    }

    fn id(&self) -> u32 {
        self.0.id()
    }

    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop();
    }
}
