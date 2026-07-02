#![forbid(unsafe_code)]

#[allow(unused_imports)]
#[path = "native_modules.rs"]
pub(crate) mod terlan_native;

#[cfg(test)]
mod http_runtime_lane;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use terlan_native::http;
use terlan_native::json;
use terlan_native::postgres::{self, Config, Pool, PostgresError};

const POSTGRES_COMMAND: &str = "native-boundary-postgres-baseline";
const HTTP_COMMAND: &str = "native-boundary-http-baseline";
const VM_COMMAND: &str = "vm-performance-baseline";
const DEFAULT_OUTPUT: &str = "../benchmarks/results/native-boundary-postgres-baseline.latest.json";
const DEFAULT_HTTP_OUTPUT: &str = "../benchmarks/results/native-boundary-http-baseline.latest.json";
const DEFAULT_VM_OUTPUT: &str = "../benchmarks/results/vm-performance-baseline.latest.json";
const DEFAULT_ITERATIONS: usize = 100;
const DEFAULT_HTTP_ITERATIONS: usize = 10_000;
const DEFAULT_HTTP_CONCURRENT_ITERATIONS: usize = 100;
const DEFAULT_VM_ITERATIONS: usize = 3;
const DEFAULT_CONCURRENCY: usize = 8;
const HTTP_CONCURRENCY_LEVELS: &[usize] = &[100, 1_000];

/// Internal benchmark entrypoint.
///
/// Inputs:
/// - First positional argument naming the benchmark.
///
/// Output:
/// - JSON report written to `TERLAN_BENCH_POSTGRES_OUTPUT` or the default
///   benchmark results path.
///
/// Transformation:
/// - Dispatches permanent benchmark harnesses without adding internal commands
///   to the public `terlc` surface.
fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some(POSTGRES_COMMAND) => run_postgres_baseline_cli(),
        Some(HTTP_COMMAND) => run_http_baseline_cli(),
        Some(VM_COMMAND) => run_vm_performance_baseline_cli(),
        Some(command) => {
            eprintln!("unsupported terlan-benchmark command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: terlan-benchmark <{POSTGRES_COMMAND}|{HTTP_COMMAND}|{VM_COMMAND}>");
            ExitCode::from(2)
        }
    }
}

/// Runs the Postgres baseline benchmark command.
///
/// Inputs:
/// - Process environment for URL, output path, iteration count, and
///   concurrency.
///
/// Output:
/// - Exit status 0 when a completed or skipped report is written.
/// - Exit status 1 when benchmark execution or report writing fails.
///
/// Transformation:
/// - Builds benchmark options, records the report, writes JSON, and prints a
///   stable one-line status for Make/CI logs.
fn run_postgres_baseline_cli() -> ExitCode {
    let options = BenchmarkOptions::from_env();
    let report = match run_postgres_baseline(&options) {
        Ok(report) => report,
        Err(error) => BenchmarkReport::failed(&options, error),
    };
    if let Err(error) = write_report(&options.output, &report) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match report.status {
        BenchmarkStatus::Completed => {
            println!(
                "[native-boundary-postgres-baseline] completed; wrote {}",
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        BenchmarkStatus::Skipped => {
            println!(
                "[native-boundary-postgres-baseline] skipped: {}; wrote {}",
                report
                    .skip_reason
                    .as_deref()
                    .unwrap_or("unknown skip reason"),
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        BenchmarkStatus::Failed => {
            eprintln!(
                "[native-boundary-postgres-baseline] failed: {}; wrote {}",
                report.error_reason.as_deref().unwrap_or("unknown error"),
                options.output.display()
            );
            ExitCode::from(1)
        }
    }
}

/// Runs the HTTP baseline benchmark command.
///
/// Inputs:
/// - Process environment for output path and iteration count.
///
/// Output:
/// - Exit status 0 when the report is written.
/// - Exit status 1 when report writing fails.
///
/// Transformation:
/// - Measures the current native HTTP response and handler dispatch surface
///   without binding a socket or starting a server.
fn run_http_baseline_cli() -> ExitCode {
    let options = HttpBenchmarkOptions::from_env();
    let report = match run_http_baseline(&options) {
        Ok(report) => report,
        Err(error) => HttpBenchmarkReport::failed(&options, error),
    };
    if let Err(error) = write_report(&options.output, &report) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match report.status {
        BenchmarkStatus::Completed => {
            println!(
                "[native-boundary-http-baseline] completed; wrote {}",
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        BenchmarkStatus::Failed => {
            eprintln!(
                "[native-boundary-http-baseline] failed: {}; wrote {}",
                report.error_reason.as_deref().unwrap_or("unknown error"),
                options.output.display()
            );
            ExitCode::from(1)
        }
        BenchmarkStatus::Skipped => {
            println!(
                "[native-boundary-http-baseline] skipped: {}; wrote {}",
                report
                    .skip_reason
                    .as_deref()
                    .unwrap_or("unknown skip reason"),
                options.output.display()
            );
            ExitCode::SUCCESS
        }
    }
}

/// Runs the VM performance baseline benchmark command.
///
/// Inputs:
/// - Process environment for output path, iteration count, and optional
///   `terlan-vm`/`terlc` binary paths.
///
/// Output:
/// - Exit status 0 when the VM baseline report is written.
/// - Exit status 1 when a completed VM track fails or report writing fails.
///
/// Transformation:
/// - Builds local binaries once, measures real VM command paths, and records
///   unavailable future runtime tracks as typed skipped rows instead of
///   pretending they were measured.
fn run_vm_performance_baseline_cli() -> ExitCode {
    let options = VmBenchmarkOptions::from_env();
    let report = match run_vm_performance_baseline(&options) {
        Ok(report) => report,
        Err(error) => VmBenchmarkReport::failed(&options, error),
    };
    if let Err(error) = write_report(&options.output, &report) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match report.status {
        BenchmarkStatus::Completed => {
            println!(
                "[vm-performance-baseline] completed; wrote {}",
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        BenchmarkStatus::Failed => {
            eprintln!(
                "[vm-performance-baseline] failed: {}; wrote {}",
                report.error_reason.as_deref().unwrap_or("unknown error"),
                options.output.display()
            );
            ExitCode::from(1)
        }
        BenchmarkStatus::Skipped => {
            println!(
                "[vm-performance-baseline] skipped: {}; wrote {}",
                report
                    .skip_reason
                    .as_deref()
                    .unwrap_or("unknown skip reason"),
                options.output.display()
            );
            ExitCode::SUCCESS
        }
    }
}

/// Benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkOptions {
    output: PathBuf,
    postgres_url: Option<String>,
    postgres_url_source: Option<&'static str>,
    iterations: usize,
    concurrency: usize,
}

/// HTTP benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpBenchmarkOptions {
    output: PathBuf,
    iterations: usize,
    concurrent_iterations: usize,
}

/// VM benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VmBenchmarkOptions {
    output: PathBuf,
    iterations: usize,
    vm_binary: Option<PathBuf>,
    compiler_binary: Option<PathBuf>,
}

impl VmBenchmarkOptions {
    /// Reads VM benchmark options from the process environment.
    ///
    /// Inputs:
    /// - `TERLAN_BENCH_VM_OUTPUT`.
    /// - `TERLAN_BENCH_VM_ITERATIONS`.
    /// - `TERLAN_BENCH_VM_BIN`.
    /// - `TERLAN_BENCH_TERLC_BIN`.
    ///
    /// Output:
    /// - Complete VM benchmark options with conservative defaults.
    ///
    /// Transformation:
    /// - Keeps the benchmark independent from installed `terlc` by allowing
    ///   callers to pin explicit local binaries.
    fn from_env() -> Self {
        Self {
            output: env::var_os("TERLAN_BENCH_VM_OUTPUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_VM_OUTPUT)),
            iterations: read_usize_var("TERLAN_BENCH_VM_ITERATIONS", DEFAULT_VM_ITERATIONS),
            vm_binary: env::var_os("TERLAN_BENCH_VM_BIN").map(PathBuf::from),
            compiler_binary: env::var_os("TERLAN_BENCH_TERLC_BIN").map(PathBuf::from),
        }
    }
}

impl HttpBenchmarkOptions {
    /// Reads HTTP benchmark options from the process environment.
    ///
    /// Inputs:
    /// - `TERLAN_BENCH_HTTP_OUTPUT`.
    /// - `TERLAN_BENCH_HTTP_ITERATIONS`.
    /// - `TERLAN_BENCH_HTTP_CONCURRENT_ITERATIONS`.
    ///
    /// Output:
    /// - Complete HTTP benchmark options with conservative defaults.
    ///
    /// Transformation:
    /// - Keeps HTTP baselines independent from Postgres-specific environment
    ///   variables.
    fn from_env() -> Self {
        Self {
            output: env::var_os("TERLAN_BENCH_HTTP_OUTPUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_HTTP_OUTPUT)),
            iterations: read_usize_var("TERLAN_BENCH_HTTP_ITERATIONS", DEFAULT_HTTP_ITERATIONS),
            concurrent_iterations: read_usize_var(
                "TERLAN_BENCH_HTTP_CONCURRENT_ITERATIONS",
                DEFAULT_HTTP_CONCURRENT_ITERATIONS,
            ),
        }
    }
}

impl BenchmarkOptions {
    /// Reads benchmark options from the process environment.
    ///
    /// Inputs:
    /// - Environment variables documented in `../benchmarks/README.md`.
    ///
    /// Output:
    /// - Complete benchmark options with conservative defaults.
    ///
    /// Transformation:
    /// - Chooses the explicit benchmark URL first, then falls back to the
    ///   existing live-test URL. Invalid numeric values fall back to defaults
    ///   instead of aborting benchmark discovery.
    fn from_env() -> Self {
        let (postgres_url, postgres_url_source) = read_url_var("TERLAN_BENCH_POSTGRES_URL")
            .map_or_else(
                || {
                    read_url_var("TERLAN_TEST_POSTGRES_URL")
                        .map(|url| (Some(url), Some("TERLAN_TEST_POSTGRES_URL")))
                        .unwrap_or((None, None))
                },
                |url| (Some(url), Some("TERLAN_BENCH_POSTGRES_URL")),
            );
        Self {
            output: env::var_os("TERLAN_BENCH_POSTGRES_OUTPUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT)),
            postgres_url,
            postgres_url_source,
            iterations: read_usize_var("TERLAN_BENCH_POSTGRES_ITERATIONS", DEFAULT_ITERATIONS),
            concurrency: read_usize_var("TERLAN_BENCH_POSTGRES_CONCURRENCY", DEFAULT_CONCURRENCY),
        }
    }
}

/// Reads a non-empty URL environment variable.
///
/// Inputs:
/// - `name`: variable name.
///
/// Output:
/// - URL string when present and non-empty.
///
/// Transformation:
/// - Trims whitespace so accidental empty variables behave like unset values.
fn read_url_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Reads a positive usize environment variable.
///
/// Inputs:
/// - `name`: variable name.
/// - `default`: fallback value.
///
/// Output:
/// - Parsed positive value or fallback.
///
/// Transformation:
/// - Keeps the harness robust in CI by ignoring malformed tuning values.
pub(crate) fn read_usize_var(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Full benchmark report serialized to JSON.
#[derive(Debug, Clone, Serialize)]
struct BenchmarkReport {
    benchmark: &'static str,
    status: BenchmarkStatus,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    adapter_stack: AdapterStack,
    postgres_url_source: Option<&'static str>,
    postgres_url_redacted: Option<String>,
    iterations: usize,
    concurrency: usize,
    measurements: Vec<Measurement>,
    assertions: Vec<AssertionResult>,
    skip_reason: Option<String>,
    error_reason: Option<String>,
}

/// HTTP benchmark report serialized to JSON.
#[derive(Debug, Clone, Serialize)]
struct HttpBenchmarkReport {
    benchmark: &'static str,
    status: BenchmarkStatus,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    adapter_stack: HttpAdapterStack,
    iterations: usize,
    concurrent_iterations_per_user: usize,
    measurements: Vec<Measurement>,
    assertions: Vec<AssertionResult>,
    skip_reason: Option<String>,
    error_reason: Option<String>,
}

/// VM performance baseline report serialized to JSON.
#[derive(Debug, Clone, Serialize)]
struct VmBenchmarkReport {
    benchmark: &'static str,
    status: BenchmarkStatus,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    runtime_stack: VmRuntimeStack,
    iterations: usize,
    measurements: Vec<Measurement>,
    assertions: Vec<AssertionResult>,
    skipped_tracks: Vec<SkippedTrack>,
    skip_reason: Option<String>,
    error_reason: Option<String>,
}

impl VmBenchmarkReport {
    /// Builds a completed VM benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `runtime_stack`: resolved binary/runtime metadata.
    /// - `measurements`: completed timing tracks.
    /// - `assertions`: correctness assertions for completed tracks.
    /// - `skipped_tracks`: future tracks with stable skip reasons.
    ///
    /// Output:
    /// - Serializable completed VM baseline report.
    ///
    /// Transformation:
    /// - Records both measured tracks and explicit unimplemented VM-owned
    ///   tracks so the report is useful as a migration baseline.
    fn completed(
        options: &VmBenchmarkOptions,
        runtime_stack: VmRuntimeStack,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
        skipped_tracks: Vec<SkippedTrack>,
    ) -> Self {
        Self {
            benchmark: VM_COMMAND,
            status: BenchmarkStatus::Completed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            runtime_stack,
            iterations: options.iterations,
            measurements,
            assertions,
            skipped_tracks,
            skip_reason: None,
            error_reason: None,
        }
    }

    /// Builds a failed VM benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `reason`: failure reason.
    ///
    /// Output:
    /// - Serializable failed report.
    ///
    /// Transformation:
    /// - Preserves metadata when a required completed VM track fails.
    fn failed(options: &VmBenchmarkOptions, reason: impl Into<String>) -> Self {
        Self {
            benchmark: VM_COMMAND,
            status: BenchmarkStatus::Failed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            runtime_stack: VmRuntimeStack::unresolved(),
            iterations: options.iterations,
            measurements: Vec::new(),
            assertions: Vec::new(),
            skipped_tracks: Vec::new(),
            skip_reason: None,
            error_reason: Some(reason.into()),
        }
    }
}

impl HttpBenchmarkReport {
    /// Builds a completed HTTP benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `measurements`: completed measurements.
    /// - `assertions`: correctness assertions.
    ///
    /// Output:
    /// - Serializable completed report.
    ///
    /// Transformation:
    /// - Adds stable toolchain and adapter metadata to the measured tracks.
    fn completed(
        options: &HttpBenchmarkOptions,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
    ) -> Self {
        Self {
            benchmark: HTTP_COMMAND,
            status: BenchmarkStatus::Completed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            adapter_stack: HttpAdapterStack::current(),
            iterations: options.iterations,
            concurrent_iterations_per_user: options.concurrent_iterations,
            measurements,
            assertions,
            skip_reason: None,
            error_reason: None,
        }
    }

    /// Builds a failed HTTP benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `reason`: failure reason.
    ///
    /// Output:
    /// - Serializable failed report.
    ///
    /// Transformation:
    /// - Preserves metadata for failed benchmark attempts.
    fn failed(options: &HttpBenchmarkOptions, reason: impl Into<String>) -> Self {
        Self {
            benchmark: HTTP_COMMAND,
            status: BenchmarkStatus::Failed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            adapter_stack: HttpAdapterStack::current(),
            iterations: options.iterations,
            concurrent_iterations_per_user: options.concurrent_iterations,
            measurements: Vec::new(),
            assertions: Vec::new(),
            skip_reason: None,
            error_reason: Some(reason.into()),
        }
    }
}

/// VM runtime stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
struct VmRuntimeStack {
    vm_binary: String,
    compiler_binary: String,
    source_execution: &'static str,
    artifact_execution: &'static str,
    skipped_track_policy: &'static str,
}

impl VmRuntimeStack {
    /// Builds resolved VM runtime metadata.
    fn resolved(vm_binary: &Path, compiler_binary: &Path) -> Self {
        Self {
            vm_binary: vm_binary.display().to_string(),
            compiler_binary: compiler_binary.display().to_string(),
            source_execution: "terlan-vm run <source.terl> --test-eval",
            artifact_execution:
                "terlc build --target terlan-vm pending; terlan-vm artifact load pending",
            skipped_track_policy: "stable skip rows for VM-owned tracks not executable yet",
        }
    }

    /// Builds unresolved VM runtime metadata for failed reports.
    fn unresolved() -> Self {
        Self {
            vm_binary: "<unresolved>".to_string(),
            compiler_binary: "<unresolved>".to_string(),
            source_execution: "terlan-vm run <source.terl> --test-eval",
            artifact_execution:
                "terlc build --target terlan-vm pending; terlan-vm artifact load pending",
            skipped_track_policy: "stable skip rows for VM-owned tracks not executable yet",
        }
    }
}

/// VM benchmark track that is required but not executable yet.
#[derive(Debug, Clone, Serialize)]
struct SkippedTrack {
    name: &'static str,
    reason: &'static str,
    detail: &'static str,
}

impl BenchmarkReport {
    /// Builds a skipped benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark configuration.
    /// - `reason`: typed skip reason.
    ///
    /// Output:
    /// - Serializable skipped report.
    ///
    /// Transformation:
    /// - Preserves environment metadata even when no database is configured.
    fn skipped(options: &BenchmarkOptions, reason: impl Into<String>) -> Self {
        Self::base(options, BenchmarkStatus::Skipped, Vec::new(), Vec::new())
            .with_skip_reason(reason)
    }

    /// Builds a failed benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark configuration.
    /// - `reason`: failure reason.
    ///
    /// Output:
    /// - Serializable failed report.
    ///
    /// Transformation:
    /// - Captures benchmark failure in the same JSON shape as completed runs.
    fn failed(options: &BenchmarkOptions, reason: impl Into<String>) -> Self {
        Self::base(options, BenchmarkStatus::Failed, Vec::new(), Vec::new())
            .with_error_reason(reason)
    }

    /// Builds the shared report header.
    ///
    /// Inputs:
    /// - Benchmark options.
    /// - Report status.
    /// - Measurements and assertions.
    ///
    /// Output:
    /// - Report with common metadata populated.
    ///
    /// Transformation:
    /// - Redacts Postgres URL credentials and records toolchain metadata.
    fn base(
        options: &BenchmarkOptions,
        status: BenchmarkStatus,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
    ) -> Self {
        Self {
            benchmark: POSTGRES_COMMAND,
            status,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            adapter_stack: AdapterStack::current(),
            postgres_url_source: options.postgres_url_source,
            postgres_url_redacted: options.postgres_url.as_deref().map(redact_postgres_url),
            iterations: options.iterations,
            concurrency: options.concurrency,
            measurements,
            assertions,
            skip_reason: None,
            error_reason: None,
        }
    }

    /// Adds a skip reason to this report.
    fn with_skip_reason(mut self, reason: impl Into<String>) -> Self {
        self.skip_reason = Some(reason.into());
        self
    }

    /// Adds an error reason to this report.
    fn with_error_reason(mut self, reason: impl Into<String>) -> Self {
        self.error_reason = Some(reason.into());
        self
    }
}

/// HTTP adapter stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
struct HttpAdapterStack {
    response_boundary: &'static str,
    request_boundary: &'static str,
    serialization: &'static str,
    socket_runtime: &'static str,
}

impl HttpAdapterStack {
    /// Returns the current HTTP adapter stack names.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Maintained crate names and boundary labels.
    ///
    /// Transformation:
    /// - Records that this first baseline measures native response/request
    ///   conversion without binding Hyper sockets.
    fn current() -> Self {
        Self {
            response_boundary: "http crate response conversion",
            request_boundary: "Terlan native HTTP request snapshot",
            serialization: "serde_json through std.data.Json adapter",
            socket_runtime: "no socket; handler/response boundary only",
        }
    }
}

/// Benchmark execution status.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchmarkStatus {
    Completed,
    Skipped,
    Failed,
}

/// Adapter stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
struct AdapterStack {
    pool: &'static str,
    postgres_client: &'static str,
    async_runtime: &'static str,
    runtime_lifecycle: &'static str,
    boundary: &'static str,
}

impl AdapterStack {
    /// Returns the current adapter stack names.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Maintained crate names and boundary label.
    ///
    /// Transformation:
    /// - Keeps the baseline explicit about which old path is measured.
    fn current() -> Self {
        Self {
            pool: "deadpool-postgres",
            postgres_client: "tokio-postgres",
            async_runtime: "tokio",
            runtime_lifecycle: "process-long OnceLock runtime",
            boundary: "SafeNative-era synchronous Rust adapter",
        }
    }
}

/// One benchmark measurement summary.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Measurement {
    name: &'static str,
    unit: &'static str,
    iterations: usize,
    total_ns: u128,
    min_ns: u128,
    mean_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    max_ns: u128,
    total_us: u128,
    min_us: u128,
    mean_us: u128,
    p50_us: u128,
    p95_us: u128,
    max_us: u128,
}

/// Correctness assertion captured next to timing data.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssertionResult {
    pub(crate) name: &'static str,
    pub(crate) passed: bool,
    pub(crate) detail: String,
}

/// Runs the Postgres baseline benchmark.
///
/// Inputs:
/// - `options`: URL, output, iteration, and concurrency configuration.
///
/// Output:
/// - Completed, skipped, or failed benchmark report.
///
/// Transformation:
/// - Connects through the current adapter, prepares a small benchmark table,
///   runs sequential and concurrent operation measurements, and validates
///   every measured operation.
fn run_postgres_baseline(options: &BenchmarkOptions) -> Result<BenchmarkReport, String> {
    let Some(url) = options.postgres_url.as_deref() else {
        return Ok(BenchmarkReport::skipped(
            options,
            "postgres_url_unconfigured",
        ));
    };
    let config = Config::new(url)
        .with_pool_limits(1, options.concurrency.max(2))
        .with_timeouts(5_000, 5_000);
    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    let (pool, connect_measurement) = measure_connect(&config)?;
    measurements.push(connect_measurement);

    prepare_benchmark_table(&pool)?;
    let query = "SELECT value FROM terlan_bench_native_boundary WHERE id = 1";
    let insert = "INSERT INTO terlan_bench_native_boundary(value) VALUES ($1)";

    measurements.push(measure_repeated(
        "query_one_select_int",
        options.iterations,
        || {
            let row = postgres::query_one(&pool, query, &[])
                .map_err(format_postgres_error)?
                .ok_or_else(|| "query_one returned no row".to_string())?;
            let value = postgres::int(&row, "value").map_err(format_postgres_error)?;
            if value == 1 {
                Ok(())
            } else {
                Err(format!("query_one returned unexpected value {value}"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "query_one_select_int",
        passed: true,
        detail: "all measured query_one calls returned value 1".to_string(),
    });

    measurements.push(measure_repeated(
        "execute_insert_param",
        options.iterations,
        || {
            let affected =
                postgres::execute(&pool, insert, &[json::int(2)]).map_err(format_postgres_error)?;
            if affected == 1 {
                Ok(())
            } else {
                Err(format!("execute affected {affected} rows"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "execute_insert_param",
        passed: true,
        detail: "all measured execute calls affected one row".to_string(),
    });

    measurements.push(measure_repeated(
        "transaction_empty_commit",
        options.iterations,
        || {
            let value =
                postgres::transaction(&pool, |_connection| Ok(1)).map_err(format_postgres_error)?;
            if value == 1 {
                Ok(())
            } else {
                Err(format!("transaction returned unexpected value {value}"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "transaction_empty_commit",
        passed: true,
        detail: "all measured transactions committed".to_string(),
    });

    measurements.push(measure_concurrent_query_one(
        &pool,
        options.concurrency,
        options.iterations,
    )?);
    assertions.push(AssertionResult {
        name: "concurrent_query_one_select_int",
        passed: true,
        detail: format!(
            "{} workers each completed {} query_one calls",
            options.concurrency, options.iterations
        ),
    });

    Ok(BenchmarkReport::base(
        options,
        BenchmarkStatus::Completed,
        measurements,
        assertions,
    ))
}

/// Runs the VM performance baseline benchmark.
///
/// Inputs:
/// - `options`: output, iteration, and optional local binary configuration.
///
/// Output:
/// - Completed VM baseline report or a failure reason.
///
/// Transformation:
/// - Resolves local `terlan-vm` and `terlc` binaries, creates deterministic
///   temporary source programs, measures supported VM paths, and records
///   explicit skipped rows for VM-owned tracks that are not executable yet.
fn run_vm_performance_baseline(options: &VmBenchmarkOptions) -> Result<VmBenchmarkReport, String> {
    let vm_binary = resolve_benchmark_binary("terlan-vm", options.vm_binary.as_deref())?;
    let compiler_binary = resolve_benchmark_binary("terlc", options.compiler_binary.as_deref())?;
    let workspace = create_vm_benchmark_workspace()?;
    let single_source = write_vm_benchmark_source(
        &workspace,
        "Single.terl",
        "module bench.Single.\n\npub main(): Bool ->\n    1 + 2 == 3.\n",
    )?;
    let project_source = write_vm_benchmark_source(
        &workspace,
        "Project.terl",
        &synthetic_helper_source("bench.Project", 20),
    )?;
    let battleship_source = write_vm_benchmark_source(
        &workspace,
        "BattleshipSized.terl",
        &synthetic_helper_source("bench.BattleshipSized", 80),
    )?;

    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    measurements.push(measure_repeated(
        "vm_binary_version_startup",
        options.iterations,
        || {
            let output = run_required_command(&vm_binary, &["--version"])?;
            require_stdout_contains("vm_binary_version_startup", &output, "terlan-vm")
        },
    )?);
    assertions.push(assertion(
        "vm_binary_version_startup",
        "terlan-vm --version completed and identified the VM binary",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_single_file",
        options.iterations,
        || run_vm_source_test(&vm_binary, &single_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_single_file",
        "single-file Terlan source compiled and returned Bool true through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_project_sized_synthetic",
        options.iterations,
        || run_vm_source_test(&vm_binary, &project_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_project_sized_synthetic",
        "synthetic project-sized source compiled and returned Bool true",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_battleship_sized_synthetic",
        options.iterations,
        || run_vm_source_test(&vm_binary, &battleship_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_battleship_sized_synthetic",
        "synthetic Battleship-sized source compiled and returned Bool true",
    ));

    Ok(VmBenchmarkReport::completed(
        options,
        VmRuntimeStack::resolved(&vm_binary, &compiler_binary),
        measurements,
        assertions,
        vm_performance_skipped_tracks(),
    ))
}

/// Runs the HTTP baseline benchmark.
///
/// Inputs:
/// - `options`: output and iteration configuration.
///
/// Output:
/// - Completed HTTP benchmark report.
///
/// Transformation:
/// - Measures static response conversion, JSON response conversion, and
///   handler-style request dispatch through the current native HTTP APIs.
fn run_http_baseline(options: &HttpBenchmarkOptions) -> Result<HttpBenchmarkReport, String> {
    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    measurements.push(measure_repeated(
        "http_static_response_to_http",
        options.iterations,
        measure_static_http_response,
    )?);
    assertions.push(AssertionResult {
        name: "http_static_response_to_http",
        passed: true,
        detail: "all static response conversions returned 200 and stable body".to_string(),
    });

    measurements.push(measure_repeated(
        "http_json_response_serialize_to_http",
        options.iterations,
        measure_json_http_response,
    )?);
    assertions.push(AssertionResult {
        name: "http_json_response_serialize_to_http",
        passed: true,
        detail: "all JSON response conversions serialized expected payload".to_string(),
    });

    let request = http::Request::from_parts_with_raw_query_metadata(
        "GET",
        "/api/counter/42",
        "",
        vec![("id".to_string(), "42".to_string())],
        "format=json",
        vec![("format".to_string(), "json".to_string())],
        vec![("accept".to_string(), "application/json".to_string())],
        Vec::new(),
    );
    measurements.push(measure_repeated(
        "http_handler_dispatch_to_response",
        options.iterations,
        || {
            let response = benchmark_http_handler(&request)?;
            let converted = response.to_http_response().map_err(format_http_error)?;
            if converted.status() != ::http::StatusCode::OK {
                return Err(format!("unexpected handler status {}", converted.status()));
            }
            if !converted.body().contains("\"id\":\"42\"") {
                return Err("unexpected handler response body".to_string());
            }
            Ok(())
        },
    )?);
    assertions.push(AssertionResult {
        name: "http_handler_dispatch_to_response",
        passed: true,
        detail: "all handler dispatch calls read request metadata and returned JSON".to_string(),
    });

    for concurrency in HTTP_CONCURRENCY_LEVELS {
        measurements.push(measure_concurrent_http_track(
            "http_static_response_to_http",
            *concurrency,
            options.concurrent_iterations,
            measure_static_http_response,
        )?);
        measurements.push(measure_concurrent_http_track(
            "http_json_response_serialize_to_http",
            *concurrency,
            options.concurrent_iterations,
            measure_json_http_response,
        )?);
        let handler_request = request.clone();
        measurements.push(measure_concurrent_http_track(
            "http_handler_dispatch_to_response",
            *concurrency,
            options.concurrent_iterations,
            move || {
                let response = benchmark_http_handler(&handler_request)?;
                let converted = response.to_http_response().map_err(format_http_error)?;
                if converted.status() != ::http::StatusCode::OK {
                    return Err(format!("unexpected handler status {}", converted.status()));
                }
                if !converted.body().contains("\"id\":\"42\"") {
                    return Err("unexpected handler response body".to_string());
                }
                Ok(())
            },
        )?);
    }
    assertions.push(AssertionResult {
        name: "http_concurrent_tracks",
        passed: true,
        detail: "static, JSON, and handler tracks completed at 100 and 1000 simulated users"
            .to_string(),
    });

    Ok(HttpBenchmarkReport::completed(
        options,
        measurements,
        assertions,
    ))
}

/// Measures one static HTTP response conversion.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when conversion preserves status and body.
///
/// Transformation:
/// - Exercises the native HTTP response constructor plus Rust `http` crate
///   conversion path.
fn measure_static_http_response() -> Result<(), String> {
    let response = http::text("Hello from Terlan", 200);
    let converted = response.to_http_response().map_err(format_http_error)?;
    if converted.status() != ::http::StatusCode::OK {
        return Err(format!("unexpected static status {}", converted.status()));
    }
    if converted.body() != "Hello from Terlan" {
        return Err("unexpected static response body".to_string());
    }
    Ok(())
}

/// Measures one JSON HTTP response conversion.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when serialization and conversion preserve expected JSON.
///
/// Transformation:
/// - Exercises `std.data.Json` adapter construction, HTTP JSON response
///   construction, and Rust `http` crate conversion.
fn measure_json_http_response() -> Result<(), String> {
    let mut payload = json::object();
    json::put(&mut payload, "status", json::string("ok")).map_err(format_json_error)?;
    json::put(&mut payload, "count", json::int(3)).map_err(format_json_error)?;
    let response = http::json(&payload, 200);
    let converted = response.to_http_response().map_err(format_http_error)?;
    if converted.status() != ::http::StatusCode::OK {
        return Err(format!("unexpected JSON status {}", converted.status()));
    }
    if !converted.body().contains("\"status\":\"ok\"") {
        return Err("unexpected JSON response body".to_string());
    }
    Ok(())
}

/// Measures a concurrent HTTP benchmark track.
///
/// Inputs:
/// - `track`: base track name.
/// - `concurrency`: number of simulated users.
/// - `iterations`: operations per user.
/// - `operation`: operation executed by every simulated user.
///
/// Output:
/// - Worker wall-time measurement summary.
///
/// Transformation:
/// - Spawns one thread per simulated user and records each worker's total
///   duration for the requested track.
fn measure_concurrent_http_track<F>(
    track: &'static str,
    concurrency: usize,
    iterations: usize,
    operation: F,
) -> Result<Measurement, String>
where
    F: Fn() -> Result<(), String> + Send + Sync + 'static,
{
    let operation = Arc::new(operation);
    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let worker_operation = Arc::clone(&operation);
        handles.push(thread::spawn(move || {
            let start = Instant::now();
            for _ in 0..iterations {
                worker_operation()?;
            }
            Ok::<Duration, String>(start.elapsed())
        }));
    }
    let mut durations = Vec::with_capacity(concurrency);
    for handle in handles {
        let duration = handle
            .join()
            .map_err(|_| "HTTP benchmark worker panicked".to_string())??;
        durations.push(duration);
    }
    Ok(Measurement::from_durations(
        concurrent_measurement_name(track, concurrency),
        &durations,
    ))
}

/// Returns the measurement name for a concurrent HTTP track.
///
/// Inputs:
/// - `track`: base track name.
/// - `concurrency`: simulated user count.
///
/// Output:
/// - Stable measurement name.
///
/// Transformation:
/// - Uses the two required concurrency levels as static names so reports do
///   not allocate leaked strings.
fn concurrent_measurement_name(track: &'static str, concurrency: usize) -> &'static str {
    match (track, concurrency) {
        ("http_static_response_to_http", 100) => {
            "http_static_response_to_http_100_users_worker_wall_time"
        }
        ("http_static_response_to_http", 1_000) => {
            "http_static_response_to_http_1000_users_worker_wall_time"
        }
        ("http_json_response_serialize_to_http", 100) => {
            "http_json_response_serialize_to_http_100_users_worker_wall_time"
        }
        ("http_json_response_serialize_to_http", 1_000) => {
            "http_json_response_serialize_to_http_1000_users_worker_wall_time"
        }
        ("http_handler_dispatch_to_response", 100) => {
            "http_handler_dispatch_to_response_100_users_worker_wall_time"
        }
        ("http_handler_dispatch_to_response", 1_000) => {
            "http_handler_dispatch_to_response_1000_users_worker_wall_time"
        }
        _ => "http_unknown_concurrent_track_worker_wall_time",
    }
}

/// Sample handler used by the HTTP baseline.
///
/// Inputs:
/// - `request`: native HTTP request snapshot.
///
/// Output:
/// - JSON response carrying route and query metadata.
///
/// Transformation:
/// - Mimics the work a Terlan handler currently performs through typed request
///   accessors and native response constructors without invoking VM.
fn benchmark_http_handler(request: &http::Request) -> Result<http::Response, String> {
    let id = http::param(request, "id").ok_or_else(|| "missing id param".to_string())?;
    let format =
        http::query(request, "format").ok_or_else(|| "missing format query".to_string())?;
    let mut payload = json::object();
    json::put(&mut payload, "id", json::string(&id)).map_err(format_json_error)?;
    json::put(&mut payload, "format", json::string(&format)).map_err(format_json_error)?;
    json::put(&mut payload, "method", json::string(&http::method(request)))
        .map_err(format_json_error)?;
    Ok(http::json(&payload, 200))
}

/// Resolves a benchmark binary path.
///
/// Inputs:
/// - `binary`: Cargo binary name.
/// - `explicit`: optional caller-supplied path.
///
/// Output:
/// - Path to an executable local binary.
///
/// Transformation:
/// - Prefers explicit configuration, then a sibling of the current benchmark
///   binary, and finally builds the requested binary once through Cargo.
fn resolve_benchmark_binary(binary: &str, explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return require_existing_binary(path);
    }
    let current = env::current_exe()
        .map_err(|error| format!("failed to resolve current benchmark binary: {error}"))?;
    let parent = current.parent().ok_or_else(|| {
        format!(
            "failed to resolve parent directory for benchmark binary `{}`",
            current.display()
        )
    })?;
    let sibling = parent.join(platform_binary_name(binary));
    if sibling.exists() {
        return Ok(sibling);
    }
    build_cargo_binary(binary)?;
    require_existing_binary(&sibling)
}

/// Returns the platform-specific binary filename.
fn platform_binary_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    }
}

/// Ensures a configured binary path exists.
fn require_existing_binary(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "benchmark binary `{}` does not exist",
            path.display()
        ))
    }
}

/// Builds one local Cargo binary needed by the benchmark.
fn build_cargo_binary(binary: &str) -> Result<(), String> {
    let output = Command::new("cargo")
        .args(["build", "-p", "terlan", "--bin", binary, "--quiet"])
        .output()
        .map_err(|error| format!("failed to start cargo build for `{binary}`: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format_command_failure("cargo build", &output))
    }
}

/// Creates a unique temporary workspace for VM benchmark sources.
fn create_vm_benchmark_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-vm-performance-baseline-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create VM benchmark workspace: {error}"))?;
    Ok(path)
}

/// Writes one Terlan benchmark source into the workspace.
fn write_vm_benchmark_source(
    workspace: &Path,
    name: &str,
    source: &str,
) -> Result<PathBuf, String> {
    let path = workspace.join(name);
    fs::write(&path, source).map_err(|error| {
        format!(
            "failed to write VM benchmark source `{}`: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

/// Builds a synthetic helper-heavy Terlan source file.
///
/// Inputs:
/// - `module`: source module name.
/// - `helper_count`: number of local helper functions.
///
/// Output:
/// - Terlan source that returns `Bool true` when all helper calls execute.
///
/// Transformation:
/// - Generates pure local calls to approximate larger source compile/run
///   workloads without depending on an external Battleship checkout.
fn synthetic_helper_source(module: &str, helper_count: usize) -> String {
    let mut source = format!("module {module}.\n\n");
    for index in 0..helper_count {
        source.push_str(&format!(
            "helper{index}(value: Int): Int ->\n    value + {index}.\n\n"
        ));
    }
    let block_size = 10;
    let block_count = helper_count.div_ceil(block_size);
    for block in 0..block_count {
        let start = block * block_size;
        let end = ((block + 1) * block_size).min(helper_count);
        let mut expression = if block == 0 {
            "1".to_string()
        } else {
            format!("block{}()", block - 1)
        };
        for index in start..end {
            expression = format!("helper{index}({expression})");
        }
        source.push_str(&format!("block{block}(): Int ->\n    {expression}.\n\n"));
    }
    let expected = 1 + helper_count.saturating_sub(1) * helper_count / 2;
    let final_call = if block_count == 0 {
        "1".to_string()
    } else {
        format!("block{}()", block_count - 1)
    };
    source.push_str(&format!(
        "pub main(): Bool ->\n    {final_call} == {expected}.\n"
    ));
    source
}

/// Runs a VM source file through `--test-eval`.
fn run_vm_source_test(vm_binary: &Path, source: &Path) -> Result<(), String> {
    let source = source.to_string_lossy().to_string();
    run_required_command(
        vm_binary,
        &["run", &source, "--entry", "main", "--test-eval"],
    )?;
    Ok(())
}

/// Captured command output for benchmark assertions.
struct BenchmarkCommandOutput {
    stdout: String,
    stderr: String,
}

/// Runs a benchmark command and requires a successful exit status.
fn run_required_command(program: &Path, args: &[&str]) -> Result<BenchmarkCommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("failed to start `{}`: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format_command_failure(
            &program.display().to_string(),
            &output,
        ));
    }
    Ok(BenchmarkCommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Requires command stdout to contain an expected string.
fn require_stdout_contains(
    track: &str,
    output: &BenchmarkCommandOutput,
    expected: &str,
) -> Result<(), String> {
    if output.stdout.contains(expected) {
        Ok(())
    } else {
        Err(format!(
            "{track} stdout did not contain `{expected}`; stdout=`{}` stderr=`{}`",
            output.stdout.trim(),
            output.stderr.trim()
        ))
    }
}

/// Formats an unsuccessful command result.
fn format_command_failure(command: &str, output: &std::process::Output) -> String {
    format!(
        "`{command}` failed with status {}; stdout=`{}` stderr=`{}`",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Builds a passed assertion record.
fn assertion(name: &'static str, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name,
        passed: true,
        detail: detail.into(),
    }
}

/// Returns required VM performance tracks that are not executable yet.
fn vm_performance_skipped_tracks() -> Vec<SkippedTrack> {
    vec![
        SkippedTrack {
            name: "vm_inspect_processes_startup",
            reason: "terlan_vm_inspect_command_pending",
            detail: "the committed terlan-vm CLI does not expose process inspection yet",
        },
        SkippedTrack {
            name: "vm_artifact_load_single_file",
            reason: "terlan_vm_artifact_load_pending",
            detail: "the committed terlan-vm CLI does not expose artifact load execution yet",
        },
        SkippedTrack {
            name: "vm_artifact_build_single_file",
            reason: "terlan_vm_build_target_pending",
            detail: "the committed terlc build command does not expose --target terlan-vm yet",
        },
        SkippedTrack {
            name: "vm_collection_operations",
            reason: "vm_collection_execution_pending",
            detail: "portable VM-owned collections are not executable through terlan-vm source run yet",
        },
        SkippedTrack {
            name: "vm_actor_send_receive",
            reason: "vm_actor_execution_pending",
            detail: "actor send/receive primitives exist as runtime tables but are not wired to source execution yet",
        },
        SkippedTrack {
            name: "vm_selective_receive",
            reason: "vm_actor_execution_pending",
            detail: "selective receive is not exposed through executable source syntax on the VM lane yet",
        },
        SkippedTrack {
            name: "vm_timer_wakeup",
            reason: "vm_timer_execution_pending",
            detail: "timer tables exist but are not driven by executable source programs yet",
        },
        SkippedTrack {
            name: "vm_native_boundary_request_lifecycle",
            reason: "vm_native_boundary_execution_pending",
            detail: "NativeBoundary lifecycle tests exist, but the VM source runner has no native call plan execution yet",
        },
        SkippedTrack {
            name: "vm_http_static_json_handler",
            reason: "vm_http_runtime_unavailable",
            detail: "VM-owned HTTP static, JSON, and handler paths are tracked by terlan-vm-http-lane-check",
        },
    ]
}

/// Measures adapter connection setup.
///
/// Inputs:
/// - `config`: Postgres adapter configuration.
///
/// Output:
/// - Connected pool and one measurement for connect time.
///
/// Transformation:
/// - Times the current adapter's `connect` call including runtime creation,
///   deadpool construction, and minimum connection warmup.
fn measure_connect(config: &Config) -> Result<(Pool, Measurement), String> {
    let start = Instant::now();
    let pool = postgres::connect(config).map_err(format_postgres_error)?;
    let duration = start.elapsed();
    Ok((
        pool,
        Measurement::from_durations("connect_pool_warmup", &[duration]),
    ))
}

/// Creates and seeds the benchmark table.
///
/// Inputs:
/// - `pool`: connected Postgres pool.
///
/// Output:
/// - Success when the table is ready.
///
/// Transformation:
/// - Uses the same adapter execute path that the benchmark later measures.
fn prepare_benchmark_table(pool: &Pool) -> Result<(), String> {
    postgres::execute(
        pool,
        "DROP TABLE IF EXISTS terlan_bench_native_boundary",
        &[],
    )
    .map_err(format_postgres_error)?;
    postgres::execute(
        pool,
        "CREATE TABLE terlan_bench_native_boundary(id BIGSERIAL PRIMARY KEY, value BIGINT NOT NULL)",
        &[],
    )
    .map_err(format_postgres_error)?;
    postgres::execute(
        pool,
        "INSERT INTO terlan_bench_native_boundary(value) VALUES (1)",
        &[],
    )
    .map_err(format_postgres_error)?;
    Ok(())
}

/// Measures a repeated synchronous adapter operation.
///
/// Inputs:
/// - `name`: measurement name.
/// - `iterations`: operation count.
/// - `operation`: fallible operation to run.
///
/// Output:
/// - Timing summary for successful operation runs.
///
/// Transformation:
/// - Records wall-clock duration for each call and stops on the first
///   correctness or adapter error.
fn measure_repeated(
    name: &'static str,
    iterations: usize,
    mut operation: impl FnMut() -> Result<(), String>,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        operation()?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(name, &durations))
}

/// Measures concurrent query_one calls through the current adapter.
///
/// Inputs:
/// - `pool`: connected Postgres pool.
/// - `concurrency`: number of worker threads.
/// - `iterations`: calls per worker.
///
/// Output:
/// - Timing summary for worker wall-clock durations.
///
/// Transformation:
/// - Clones the adapter pool handle into OS threads. Each call still uses the
///   current synchronous Rust/Tokio adapter path, including per-call runtime
///   creation inside the adapter.
fn measure_concurrent_query_one(
    pool: &Pool,
    concurrency: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let worker_pool = pool.clone();
        handles.push(thread::spawn(move || {
            let start = Instant::now();
            for _ in 0..iterations {
                let row = postgres::query_one(
                    &worker_pool,
                    "SELECT value FROM terlan_bench_native_boundary WHERE id = 1",
                    &[],
                )
                .map_err(format_postgres_error)?
                .ok_or_else(|| "query_one returned no row".to_string())?;
                let value = postgres::int(&row, "value").map_err(format_postgres_error)?;
                if value != 1 {
                    return Err(format!("query_one returned unexpected value {value}"));
                }
            }
            Ok(start.elapsed())
        }));
    }
    let mut durations = Vec::with_capacity(concurrency);
    for handle in handles {
        let duration = handle
            .join()
            .map_err(|_| "concurrent query worker panicked".to_string())??;
        durations.push(duration);
    }
    Ok(Measurement::from_durations(
        "concurrent_query_one_select_int_worker_wall_time",
        &durations,
    ))
}

impl Measurement {
    /// Builds a timing summary from raw durations.
    ///
    /// Inputs:
    /// - `name`: measurement name.
    /// - `durations`: non-empty operation durations.
    ///
    /// Output:
    /// - Nanosecond and microsecond summaries with min, mean, p50, p95, and
    ///   max.
    ///
    /// Transformation:
    /// - Sorts copied duration values and uses nearest-rank percentiles. The
    ///   nanosecond fields keep very small HTTP adapter operations visible;
    ///   the microsecond fields stay useful for slower database and worker
    ///   wall-time paths.
    pub(crate) fn from_durations(name: &'static str, durations: &[Duration]) -> Self {
        let mut values_ns = durations.iter().map(Duration::as_nanos).collect::<Vec<_>>();
        values_ns.sort_unstable();
        let total_ns = values_ns.iter().sum::<u128>();
        let mut values_us = durations
            .iter()
            .map(Duration::as_micros)
            .collect::<Vec<_>>();
        values_us.sort_unstable();
        let total_us = values_us.iter().sum::<u128>();
        let iterations = values_ns.len();
        Self {
            name,
            unit: "microseconds",
            iterations,
            total_ns,
            min_ns: values_ns[0],
            mean_ns: total_ns / iterations as u128,
            p50_ns: percentile(&values_ns, 50),
            p95_ns: percentile(&values_ns, 95),
            max_ns: values_ns[iterations - 1],
            total_us,
            min_us: values_us[0],
            mean_us: total_us / iterations as u128,
            p50_us: percentile(&values_us, 50),
            p95_us: percentile(&values_us, 95),
            max_us: values_us[iterations - 1],
        }
    }
}

/// Returns a nearest-rank percentile from sorted microsecond values.
///
/// Inputs:
/// - `sorted_values`: sorted non-empty duration values.
/// - `percentile_value`: percentile from 0 to 100.
///
/// Output:
/// - Value at the selected percentile rank.
///
/// Transformation:
/// - Uses integer arithmetic to avoid floating-point rounding in reports.
fn percentile(sorted_values: &[u128], percentile_value: usize) -> u128 {
    let max_index = sorted_values.len() - 1;
    let index = (max_index * percentile_value).div_ceil(100);
    sorted_values[index]
}

/// Formats a stable Postgres adapter error.
///
/// Inputs:
/// - `error`: Postgres adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves the portable error code while keeping driver text available for
///   benchmark failure diagnostics.
fn format_postgres_error(error: PostgresError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

/// Formats a stable HTTP adapter error.
///
/// Inputs:
/// - `error`: HTTP adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves the portable error code for benchmark failure diagnostics.
fn format_http_error(error: http::HttpError) -> String {
    format!(
        "error[{}]: {} (status {})",
        error.code(),
        error.message(),
        error.status()
    )
}

/// Formats a stable JSON adapter error.
///
/// Inputs:
/// - `error`: JSON adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves JSON adapter diagnostics for HTTP benchmark failures.
fn format_json_error(error: json::JsonError) -> String {
    format!(
        "error[{}]: {} (offset {})",
        error.code(),
        error.message(),
        error.offset()
    )
}

/// Writes a benchmark report as pretty JSON.
///
/// Inputs:
/// - `path`: output path.
/// - `report`: report to serialize.
///
/// Output:
/// - Success when the report is written.
///
/// Transformation:
/// - Creates parent directories and serializes stable JSON for checked
///   baseline storage.
pub(crate) fn write_report<T: Serialize>(path: &Path, report: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("{}: failed to create directory: {error}", parent.display())
        })?;
    }
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("failed to serialize benchmark report: {error}"))?;
    fs::write(path, format!("{json}\n")).map_err(|error| {
        format!(
            "{}: failed to write benchmark report: {error}",
            path.display()
        )
    })
}

/// Returns a Unix timestamp in seconds.
///
/// Inputs:
/// - Current system clock.
///
/// Output:
/// - Seconds since Unix epoch, or zero if the system clock is before epoch.
///
/// Transformation:
/// - Keeps report timestamps machine-readable without adding date/time
///   dependencies.
pub(crate) fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Returns a Unix timestamp in nanoseconds.
///
/// Inputs:
/// - Current system clock.
///
/// Output:
/// - Nanoseconds since Unix epoch, or zero if the system clock is before epoch.
///
/// Transformation:
/// - Provides a cheap unique suffix for temporary benchmark workspaces.
fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// Reads the installed Rust compiler version.
///
/// Inputs:
/// - Local `rustc` executable on PATH.
///
/// Output:
/// - Version string when available.
///
/// Transformation:
/// - Shells out once for report metadata and tolerates missing toolchain text.
pub(crate) fn rustc_version() -> Option<String> {
    let output = Command::new("rustc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Redacts credentials from a Postgres URL.
///
/// Inputs:
/// - `url`: original Postgres URL.
///
/// Output:
/// - Redacted URL string, or a stable placeholder when parsing fails.
///
/// Transformation:
/// - Uses the `url` crate rather than string slicing so credentials are not
///   accidentally leaked in benchmark reports.
fn redact_postgres_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return "<invalid postgres url>".to_string();
    };
    let _ = parsed.set_username(if parsed.username().is_empty() {
        ""
    } else {
        "redacted"
    });
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("redacted"));
    }
    parsed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies synthetic VM sources scale helper count and expected result.
    ///
    /// Inputs:
    /// - Module name and helper count.
    ///
    /// Output:
    /// - Test passes when generated source contains the requested helpers and
    ///   stable assertion expression.
    ///
    /// Transformation:
    /// - Protects the project/Battleship-sized benchmark source generator from
    ///   silently shrinking to a tiny single-expression workload.
    #[test]
    fn synthetic_helper_source_contains_requested_workload() {
        let source = synthetic_helper_source("bench.Sample", 5);

        assert!(source.contains("module bench.Sample."));
        assert!(source.contains("helper0(value: Int): Int"));
        assert!(source.contains("helper4(value: Int): Int"));
        assert!(source.contains("block0(): Int"));
        assert!(source.contains("helper4(helper3(helper2(helper1(helper0(1)))))"));
        assert!(source.contains("pub main(): Bool ->\n    block0() == 11."));
    }

    /// Verifies the VM performance baseline records all required pending lanes.
    ///
    /// Inputs:
    /// - Static skipped-track list.
    ///
    /// Output:
    /// - Test passes when the skipped tracks cover the roadmap-required
    ///   runtime categories.
    ///
    /// Transformation:
    /// - Keeps unimplemented VM tracks visible in the report instead of
    ///   letting them disappear from the benchmark surface.
    #[test]
    fn vm_performance_skipped_tracks_cover_runtime_gaps() {
        let tracks = vm_performance_skipped_tracks();
        let names = tracks.iter().map(|track| track.name).collect::<Vec<_>>();

        assert!(names.contains(&"vm_collection_operations"));
        assert!(names.contains(&"vm_actor_send_receive"));
        assert!(names.contains(&"vm_selective_receive"));
        assert!(names.contains(&"vm_timer_wakeup"));
        assert!(names.contains(&"vm_native_boundary_request_lifecycle"));
        assert!(names.contains(&"vm_http_static_json_handler"));
        assert!(tracks.iter().all(|track| !track.reason.is_empty()));
        assert!(tracks.iter().all(|track| !track.detail.is_empty()));
    }
}
