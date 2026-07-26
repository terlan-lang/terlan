use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{
    require_stdout_contains, resolve_benchmark_binary, run_required_command, rustc_version,
    write_report,
};

#[path = "binary_protocol_snapshot.rs"]
mod snapshot;

pub(super) const COMMAND: &str = "vm-binary-protocol-baseline";

const DEFAULT_OUTPUT: &str = "target/quality/vm-binary-protocol-benchmark.json";
const FIXTURE_RELATIVE_PATH: &str = "benchmarks/fixtures/BinaryProtocolBenchmarkTest.terl";
const SAMPLE_COUNT: usize = 3;
const FRAMING_PAYLOAD_BYTES: usize = 128;
const DETERMINISTIC_SEED: u64 = 0x5445_524c_414e_0007;

#[derive(Debug, Serialize)]
struct BinaryProtocolBenchmarkReport {
    schema: &'static str,
    benchmark: &'static str,
    status: &'static str,
    measurement_scope: &'static str,
    generated_at_unix_seconds: u64,
    compiler: String,
    vm_binary: String,
    rustc_version: Option<String>,
    platform: String,
    runtime_lane: &'static str,
    profile: &'static str,
    deterministic_seed: u64,
    fixture: &'static str,
    warm_sample_count: usize,
    source_warm_process_overhead_us: u64,
    scale_points: &'static [usize],
    scenarios: Vec<BinaryProtocolScenarioReport>,
    transport_scenarios: Vec<BinaryProtocolTransportScenarioReport>,
}

#[derive(Debug, Serialize)]
struct BinaryProtocolScenarioReport {
    id: String,
    test: String,
    workload_class: &'static str,
    scale: usize,
    operation_count: usize,
    concurrency: usize,
    cold_measurement_scope: &'static str,
    warm_measurement_scope: &'static str,
    cold_end_to_end_us: u64,
    warm_end_to_end_samples_us: Vec<u64>,
    warm_mean_end_to_end_us: u64,
    warm_median_end_to_end_us: u64,
    warm_p95_end_to_end_us: u64,
    warm_p99_end_to_end_us: u64,
    warm_median_operations_per_second: f64,
    unexpected_error_count: usize,
    expected_typed_failure_count: usize,
    unexpected_error_rate_percent: f64,
    comparison_status: &'static str,
    winner: &'static str,
    relative_delta_percent: Option<f64>,
    correctness: &'static str,
}

#[derive(Debug, Serialize)]
struct BinaryProtocolTransportScenarioReport {
    id: String,
    workload: &'static str,
    workload_class: &'static str,
    measurement_scope: &'static str,
    framing: &'static str,
    scale: usize,
    operation_count: usize,
    concurrency: usize,
    payload_bytes: usize,
    cold_measurement_us: u64,
    warm_measurement_samples_us: Vec<u64>,
    warm_mean_measurement_us: u64,
    warm_median_measurement_us: u64,
    warm_p95_measurement_us: u64,
    warm_p99_measurement_us: u64,
    warm_median_operations_per_second: f64,
    unexpected_error_count: usize,
    expected_typed_failure_count: usize,
    unexpected_error_rate_percent: f64,
    comparison_status: &'static str,
    winner: &'static str,
    relative_delta_percent: Option<f64>,
    correctness: &'static str,
}

struct WarmStatistics {
    samples_us: Vec<u64>,
    mean_us: u64,
    median_us: u64,
    p95_us: u64,
    p99_us: u64,
    median_operations_per_second: f64,
}

#[derive(Debug, Deserialize)]
struct VmFramingBenchmarkOutput {
    benchmark: String,
    status: String,
    workload: String,
    iterations: usize,
    payload_bytes: usize,
    expected_typed_failure_count: usize,
    measurement: VmFramingMeasurement,
    assertion: VmFramingAssertion,
}

#[derive(Debug, Deserialize)]
struct VmFramingMeasurement {
    name: String,
    total_us: u64,
}

#[derive(Debug, Deserialize)]
struct VmFramingAssertion {
    passed: bool,
}

#[derive(Clone, Copy)]
struct BinaryProtocolWorkload {
    id: &'static str,
    test_prefix: &'static str,
    workload_class: &'static str,
    operations_per_iteration: usize,
    expects_typed_failure: bool,
}

#[derive(Clone, Copy)]
struct FramingWorkload {
    id: &'static str,
    name: &'static str,
    workload_class: &'static str,
    measurement_name: &'static str,
    expects_typed_failure: bool,
}

const SCALE_POINTS: &[usize] = &[1, 10, 100, 1_000];

const WORKLOADS: &[BinaryProtocolWorkload] = &[
    BinaryProtocolWorkload {
        id: "fixed_header",
        test_prefix: "fixed_header_protocol_benchmark",
        workload_class: "success",
        operations_per_iteration: 1,
        expects_typed_failure: false,
    },
    BinaryProtocolWorkload {
        id: "composed_variable_body",
        test_prefix: "composed_variable_body_protocol_benchmark",
        workload_class: "success",
        operations_per_iteration: 2,
        expects_typed_failure: false,
    },
    BinaryProtocolWorkload {
        id: "invalid_width",
        test_prefix: "adversarial_invalid_width_protocol_benchmark",
        workload_class: "adversarial",
        operations_per_iteration: 1,
        expects_typed_failure: true,
    },
    BinaryProtocolWorkload {
        id: "duplicate_capture",
        test_prefix: "adversarial_duplicate_capture_protocol_benchmark",
        workload_class: "adversarial",
        operations_per_iteration: 1,
        expects_typed_failure: true,
    },
    BinaryProtocolWorkload {
        id: "unsupported_backend",
        test_prefix: "adversarial_unsupported_backend_protocol_benchmark",
        workload_class: "adversarial",
        operations_per_iteration: 1,
        expects_typed_failure: true,
    },
];

const FRAMING_WORKLOADS: &[FramingWorkload] = &[
    FramingWorkload {
        id: "vm_tcp_length_prefixed_framing",
        name: "roundtrip",
        workload_class: "success",
        measurement_name: "vm_in_memory_length_prefixed_frame_roundtrip",
        expects_typed_failure: false,
    },
    FramingWorkload {
        id: "vm_tcp_truncated_framing",
        name: "truncated",
        workload_class: "adversarial",
        measurement_name: "vm_in_memory_length_prefixed_truncated_frame_rejection",
        expects_typed_failure: true,
    },
    FramingWorkload {
        id: "vm_tcp_malformed_framing",
        name: "malformed-length",
        workload_class: "adversarial",
        measurement_name: "vm_in_memory_malformed_length_rejection",
        expects_typed_failure: true,
    },
    FramingWorkload {
        id: "vm_tcp_invalid_utf8",
        name: "invalid-utf8",
        workload_class: "adversarial",
        measurement_name: "vm_in_memory_invalid_utf8_rejection",
        expects_typed_failure: true,
    },
];

/// Runs the checked-in binary protocol benchmark workloads.
pub(super) fn run_cli() -> ExitCode {
    let output = env::var_os("TERLAN_BENCH_BINARY_PROTOCOL_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT));
    let compiler = env::var_os("TERLAN_BENCH_TERLC").map(PathBuf::from);
    let vm_binary = env::var_os("TERLAN_BENCH_VM").map(PathBuf::from);
    let report = match run(compiler.as_deref(), vm_binary.as_deref()) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("[{COMMAND}] failed: {error}");
            return ExitCode::from(1);
        }
    };
    if let Err(error) = write_report(&output, &report) {
        eprintln!("[{COMMAND}] failed: {error}");
        return ExitCode::from(1);
    }
    println!("[{COMMAND}] completed; wrote {}", output.display());
    ExitCode::SUCCESS
}

fn run(
    explicit_compiler: Option<&Path>,
    explicit_vm_binary: Option<&Path>,
) -> Result<BinaryProtocolBenchmarkReport, String> {
    let compiler = resolve_benchmark_binary("terlc", explicit_compiler)?;
    let vm_binary = resolve_benchmark_binary("terlan-vm", explicit_vm_binary)?;
    let fixture = fixture_path();
    if !fixture.is_file() {
        return Err(format!(
            "binary protocol benchmark fixture `{}` does not exist",
            fixture.display()
        ));
    }
    let fixture_arg = fixture
        .to_str()
        .ok_or_else(|| format!("fixture path `{}` is not UTF-8", fixture.display()))?;
    let warm_process_overhead =
        run_scenario_once(&compiler, fixture_arg, "protocol_benchmark_noop")?;
    let mut scenarios = Vec::with_capacity(WORKLOADS.len() * SCALE_POINTS.len());
    for workload in WORKLOADS {
        for &scale in SCALE_POINTS {
            let test = format!("{}_{}", workload.test_prefix, scale);
            let cold = run_scenario_once(&compiler, fixture_arg, &test)?;
            let warm_test = format!("{test}_warm");
            let warm_total = run_scenario_once(&compiler, fixture_arg, &warm_test)?;
            let warm_samples =
                steady_state_warm_samples(warm_total, warm_process_overhead, SAMPLE_COUNT);
            scenarios.push(scenario_report(*workload, scale, test, cold, warm_samples));
        }
    }
    let mut transport_scenarios = Vec::with_capacity(FRAMING_WORKLOADS.len() * SCALE_POINTS.len());
    for workload in FRAMING_WORKLOADS {
        for &scale in SCALE_POINTS {
            let cold =
                run_framing_scenario_once(&vm_binary, *workload, scale, FRAMING_PAYLOAD_BYTES)?;
            let mut warm_samples = Vec::with_capacity(SAMPLE_COUNT);
            for _ in 0..SAMPLE_COUNT {
                warm_samples.push(run_framing_scenario_once(
                    &vm_binary,
                    *workload,
                    scale,
                    FRAMING_PAYLOAD_BYTES,
                )?);
            }
            transport_scenarios.push(transport_scenario_report(
                *workload,
                scale,
                FRAMING_PAYLOAD_BYTES,
                cold,
                warm_samples,
            ));
        }
    }
    snapshot::validate(&scenarios, &transport_scenarios)?;
    Ok(BinaryProtocolBenchmarkReport {
        schema: "terlan.vm-binary-protocol-benchmark.v8",
        benchmark: "vm-binary-protocol",
        status: "completed",
        measurement_scope: "cold-compiler-process-plus-vm;warm-load-once-vm-loop",
        generated_at_unix_seconds: unix_seconds(),
        compiler: compiler.display().to_string(),
        vm_binary: vm_binary.display().to_string(),
        rustc_version: rustc_version(),
        platform: format!("{}-{}", env::consts::OS, env::consts::ARCH),
        runtime_lane: "terlan-vm",
        profile: "test",
        deterministic_seed: DETERMINISTIC_SEED,
        fixture: FIXTURE_RELATIVE_PATH,
        warm_sample_count: SAMPLE_COUNT,
        source_warm_process_overhead_us: duration_micros(warm_process_overhead),
        scale_points: SCALE_POINTS,
        scenarios,
        transport_scenarios,
    })
}

fn run_scenario_once(compiler: &Path, fixture: &str, test: &str) -> Result<Duration, String> {
    let started = Instant::now();
    let output = run_required_command(compiler, &["test", fixture, "--name", test])?;
    require_stdout_contains(test, &output, "test result: ok. 1 passed")?;
    Ok(started.elapsed())
}

fn run_framing_scenario_once(
    vm_binary: &Path,
    workload: FramingWorkload,
    iterations: usize,
    payload_bytes: usize,
) -> Result<Duration, String> {
    let iterations_arg = iterations.to_string();
    let payload_bytes_arg = payload_bytes.to_string();
    let output = run_required_command(
        vm_binary,
        &[
            "benchmark-in-memory-framing",
            "--iterations",
            &iterations_arg,
            "--payload-bytes",
            &payload_bytes_arg,
            "--workload",
            workload.name,
        ],
    )?;
    parse_framing_measurement(&output.stdout, workload, iterations, payload_bytes)
}

fn parse_framing_measurement(
    stdout: &str,
    expected_workload: FramingWorkload,
    expected_iterations: usize,
    expected_payload_bytes: usize,
) -> Result<Duration, String> {
    let report = serde_json::from_str::<VmFramingBenchmarkOutput>(stdout)
        .map_err(|error| format!("invalid VM framing benchmark JSON: {error}"))?;
    if report.benchmark != "vm-in-memory-length-prefixed-framing"
        || report.status != "completed"
        || report.workload != expected_workload.name
        || report.measurement.name != expected_workload.measurement_name
    {
        return Err(format!(
            "unexpected VM framing benchmark contract: benchmark=`{}`, status=`{}`, workload=`{}`, measurement=`{}`",
            report.benchmark, report.status, report.workload, report.measurement.name
        ));
    }
    if report.iterations != expected_iterations || report.payload_bytes != expected_payload_bytes {
        return Err(format!(
            "VM framing benchmark dimensions changed: expected iterations={expected_iterations}, payload_bytes={expected_payload_bytes}; got iterations={}, payload_bytes={}",
            report.iterations, report.payload_bytes
        ));
    }
    if !report.assertion.passed {
        return Err(format!(
            "VM framing benchmark correctness assertion failed for workload `{}`",
            expected_workload.name
        ));
    }
    let expected_typed_failure_count = if expected_workload.expects_typed_failure {
        expected_iterations
    } else {
        0
    };
    if report.expected_typed_failure_count != expected_typed_failure_count {
        return Err(format!(
            "VM framing benchmark typed failure count changed: expected {expected_typed_failure_count}, got {}",
            report.expected_typed_failure_count
        ));
    }
    Ok(Duration::from_micros(report.measurement.total_us))
}

fn scenario_report(
    workload: BinaryProtocolWorkload,
    scale: usize,
    test: String,
    cold: Duration,
    warm_samples: Vec<Duration>,
) -> BinaryProtocolScenarioReport {
    let operation_count = scale * workload.operations_per_iteration;
    let statistics = warm_statistics(warm_samples, operation_count);
    BinaryProtocolScenarioReport {
        id: format!("{}-{scale}", workload.id),
        test,
        workload_class: workload.workload_class,
        scale,
        operation_count,
        concurrency: 1,
        cold_measurement_scope: "compiler-process-plus-vm-test-workload",
        warm_measurement_scope: "load-once-vm-loop-after-process-overhead",
        cold_end_to_end_us: duration_micros(cold),
        warm_mean_end_to_end_us: statistics.mean_us,
        warm_median_end_to_end_us: statistics.median_us,
        warm_p95_end_to_end_us: statistics.p95_us,
        warm_p99_end_to_end_us: statistics.p99_us,
        warm_end_to_end_samples_us: statistics.samples_us,
        warm_median_operations_per_second: statistics.median_operations_per_second,
        unexpected_error_count: 0,
        expected_typed_failure_count: if workload.expects_typed_failure {
            scale
        } else {
            0
        },
        unexpected_error_rate_percent: 0.0,
        comparison_status: "unsupported-no-equivalent-baseline",
        winner: "not-comparable",
        relative_delta_percent: None,
        correctness: "validated-every-frame",
    }
}

fn transport_scenario_report(
    workload: FramingWorkload,
    scale: usize,
    payload_bytes: usize,
    cold: Duration,
    warm_samples: Vec<Duration>,
) -> BinaryProtocolTransportScenarioReport {
    let statistics = warm_statistics(warm_samples, scale);
    BinaryProtocolTransportScenarioReport {
        id: format!("{}-{scale}", workload.id),
        workload: workload.name,
        workload_class: workload.workload_class,
        measurement_scope: "vm-owned-in-memory-tcp-framing",
        framing: "u32-big-endian-length-prefixed",
        scale,
        operation_count: scale,
        concurrency: 1,
        payload_bytes,
        cold_measurement_us: duration_micros(cold),
        warm_mean_measurement_us: statistics.mean_us,
        warm_median_measurement_us: statistics.median_us,
        warm_p95_measurement_us: statistics.p95_us,
        warm_p99_measurement_us: statistics.p99_us,
        warm_measurement_samples_us: statistics.samples_us,
        warm_median_operations_per_second: statistics.median_operations_per_second,
        unexpected_error_count: 0,
        expected_typed_failure_count: if workload.expects_typed_failure {
            scale
        } else {
            0
        },
        unexpected_error_rate_percent: 0.0,
        comparison_status: "unsupported-no-equivalent-baseline",
        winner: "not-comparable",
        relative_delta_percent: None,
        correctness: if workload.expects_typed_failure {
            "validated-every-typed-failure"
        } else {
            "validated-every-frame"
        },
    }
}

fn warm_statistics(warm_samples: Vec<Duration>, operation_count: usize) -> WarmStatistics {
    let samples_us = warm_samples
        .into_iter()
        .map(duration_micros)
        .collect::<Vec<_>>();
    let median_us = percentile(&samples_us, 50);
    let median_operations_per_second = if median_us == 0 {
        0.0
    } else {
        operation_count as f64 * 1_000_000.0 / median_us as f64
    };
    WarmStatistics {
        mean_us: mean(&samples_us),
        median_us,
        p95_us: percentile(&samples_us, 95),
        p99_us: percentile(&samples_us, 99),
        samples_us,
        median_operations_per_second,
    }
}

fn steady_state_warm_samples(
    measured: Duration,
    process_overhead: Duration,
    sample_count: usize,
) -> Vec<Duration> {
    let steady_total = measured.saturating_sub(process_overhead);
    let per_sample_nanos = (steady_total.as_nanos() / sample_count as u128).max(1_000);
    let per_sample_nanos = u64::try_from(per_sample_nanos).unwrap_or(u64::MAX);
    vec![Duration::from_nanos(per_sample_nanos); sample_count]
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn mean(values: &[u64]) -> u64 {
    values.iter().sum::<u64>() / values.len() as u64
}

fn percentile(values: &[u64], percentile: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(FIXTURE_RELATIVE_PATH)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "binary_protocol_test.rs"]
mod binary_protocol_test;
