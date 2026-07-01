#![forbid(unsafe_code)]

#[allow(unused_imports)]
#[path = "native_modules.rs"]
pub(crate) mod terlan_native;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use terlan_native::json;
use terlan_native::postgres::{self, Config, Pool, PostgresError};

const COMMAND: &str = "native-boundary-postgres-baseline";
const DEFAULT_OUTPUT: &str = "../benchmarks/results/native-boundary-postgres-baseline.latest.json";
const DEFAULT_ITERATIONS: usize = 100;
const DEFAULT_CONCURRENCY: usize = 8;

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
        Some(COMMAND) => run_postgres_baseline_cli(),
        Some(command) => {
            eprintln!("unsupported terlan-benchmark command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("usage: terlan-benchmark {COMMAND}");
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

/// Benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkOptions {
    output: PathBuf,
    postgres_url: Option<String>,
    postgres_url_source: Option<&'static str>,
    iterations: usize,
    concurrency: usize,
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
fn read_usize_var(name: &str, default: usize) -> usize {
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
            benchmark: COMMAND,
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

/// Benchmark execution status.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum BenchmarkStatus {
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
struct Measurement {
    name: &'static str,
    unit: &'static str,
    iterations: usize,
    total_us: u128,
    min_us: u128,
    mean_us: u128,
    p50_us: u128,
    p95_us: u128,
    max_us: u128,
}

/// Correctness assertion captured next to timing data.
#[derive(Debug, Clone, Serialize)]
struct AssertionResult {
    name: &'static str,
    passed: bool,
    detail: String,
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
    /// - Microsecond summary with min, mean, p50, p95, and max.
    ///
    /// Transformation:
    /// - Sorts copied duration values and uses nearest-rank percentiles.
    fn from_durations(name: &'static str, durations: &[Duration]) -> Self {
        let mut values = durations
            .iter()
            .map(Duration::as_micros)
            .collect::<Vec<_>>();
        values.sort_unstable();
        let total_us = values.iter().sum::<u128>();
        let iterations = values.len();
        Self {
            name,
            unit: "microseconds",
            iterations,
            total_us,
            min_us: values[0],
            mean_us: total_us / iterations as u128,
            p50_us: percentile(&values, 50),
            p95_us: percentile(&values, 95),
            max_us: values[iterations - 1],
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
fn write_report(path: &Path, report: &BenchmarkReport) -> Result<(), String> {
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
fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
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
fn rustc_version() -> Option<String> {
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
