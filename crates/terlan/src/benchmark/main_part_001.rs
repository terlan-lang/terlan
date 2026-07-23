use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use terlan_native::http;
use terlan_native::json;
use terlan_native::postgres::{self, Config, Pool, PostgresError};
pub(crate) use vm_runtime::{
    actor, map_value, memory, process, resource, scheduler, table, timer, ReplValue,
};

use actor::{VmActorReceive, VmActorRuntime};
use process::{VmProcessSource, VmProcessTable};
use resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy};
use scheduler::{VmScheduler, VmSchedulerDecision, VmSchedulerOutcome};
use table::{VmTableAccess, VmTableStore};
use timer::{VmTimerKind, VmTimerTable};
use ReplValue as VmPrimitiveValue;

pub(crate) mod runtime {
    pub(crate) mod native_image {
        pub(crate) use crate::boundary_type::TvmBoundaryType;
    }

    pub(crate) mod vm {
        #[cfg(test)]
        pub(crate) use crate::actor;
        #[cfg(test)]
        pub(crate) use crate::memory;
        #[cfg(test)]
        pub(crate) use crate::persistent_actor::{distributed_state, distributed_storage};
        pub(crate) use crate::process;
        #[cfg(test)]
        pub(crate) use crate::resource;
        pub(crate) use crate::scheduler;
        pub(crate) use crate::timer;
        pub(crate) use crate::vm_runtime::native_boundary;
        pub(crate) use crate::vm_runtime::native_image_diagnostics;
        pub(crate) use crate::vm_runtime::postgres;
        #[cfg(test)]
        pub(crate) use crate::vm_runtime::reference;
        pub(crate) use crate::ReplValue;
    }
}

const POSTGRES_COMMAND: &str = "native-boundary-postgres-baseline";
const HTTP_COMMAND: &str = "native-boundary-http-baseline";
const VM_COMMAND: &str = "vm-performance-baseline";
const VM_HTTP_COMMAND: &str = "vm-http-runtime-baseline";
const DEFAULT_OUTPUT: &str = "../benchmarks/results/native-boundary-postgres-baseline.latest.json";
const DEFAULT_HTTP_OUTPUT: &str = "../benchmarks/results/native-boundary-http-baseline.latest.json";
const DEFAULT_VM_OUTPUT: &str = "../benchmarks/results/vm-performance-baseline.latest.json";
const DEFAULT_VM_HTTP_OUTPUT: &str = "../benchmarks/results/vm-http-runtime-baseline.latest.json";
const DEFAULT_ITERATIONS: usize = 100;
const DEFAULT_HTTP_ITERATIONS: usize = 10_000;
const DEFAULT_VM_HTTP_ITERATIONS: usize = 25;
const DEFAULT_HTTP_CONCURRENT_ITERATIONS: usize = 100;
const DEFAULT_VM_ITERATIONS: usize = 3;
const DEFAULT_CONCURRENCY: usize = 8;
const HTTP_CONCURRENCY_LEVELS: &[usize] = &[100, 1_000];
const MAP_BENCHMARK_SIZES: &[usize] = &[16, 32, 33, 127, 128, 129, 5_000];
const MAP_STRESS_SIZE: usize = 5_000;
const COLLISION_HEAVY_MAP_SIZE: usize = 512;

/// Internal benchmark entrypoint.
///
/// Inputs:
/// - First positional argument naming the benchmark.
/// Output:
/// - JSON report written to `TERLAN_BENCH_POSTGRES_OUTPUT` or the default
///   benchmark results path.
/// Transformation:
/// - Dispatches permanent benchmark harnesses without adding internal commands
///   to the public `terlc` surface.
fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some(POSTGRES_COMMAND) => run_postgres_baseline_cli(),
        Some(HTTP_COMMAND) => run_http_baseline_cli(),
        Some(VM_COMMAND) => run_vm_performance_baseline_cli(),
        Some(VM_HTTP_COMMAND) => run_vm_http_runtime_baseline_cli(),
        Some(binary_protocol::COMMAND) => binary_protocol::run_cli(),
        Some(persistent_actor::COMMAND) => persistent_actor::run_cli(),
        Some(runtime_workloads::COMMAND) => runtime_workloads::run_cli(),
        Some(aot_compilation::COMMAND) => aot_compilation::run_cli(),
        Some(aot_compilation::SELF_TEST_COMMAND) => aot_compilation::run_self_test_cli(),
        Some(aot_compilation::VALIDATE_COMMAND) => aot_compilation::run_validate_cli(),
        Some(http_aot_performance::COMMAND) => http_aot_performance::run_cli(),
        Some(http_aot_performance::COMPARE_COMMAND) => http_aot_performance::run_compare_cli(),
        Some(http_aot_performance::SELF_TEST_COMMAND) => http_aot_performance::run_self_test_cli(),
        Some(command) => {
            eprintln!("unsupported terlan-benchmark command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!(
                "usage: terlan-benchmark <{POSTGRES_COMMAND}|{HTTP_COMMAND}|{VM_COMMAND}|{VM_HTTP_COMMAND}|{}|{}|{}|{}|{}|{}|{}|{}|{}>",
                binary_protocol::COMMAND,
                persistent_actor::COMMAND,
                runtime_workloads::COMMAND,
                aot_compilation::COMMAND,
                aot_compilation::SELF_TEST_COMMAND,
                aot_compilation::VALIDATE_COMMAND,
                http_aot_performance::COMMAND,
                http_aot_performance::COMPARE_COMMAND,
                http_aot_performance::SELF_TEST_COMMAND
            );
            ExitCode::from(2)
        }
    }
}

/// Runs the Postgres baseline benchmark command.
///
/// Inputs:
/// - Process environment for URL, output path, iteration count, and
///   concurrency.
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

/// Runs the VM-backed HTTP runtime benchmark command.
///
/// Inputs:
/// - Process environment for output path, iteration count, and optional
///   `terlc` binary path.
///
/// Output:
/// - Exit status 0 when a real HTTP VM-handler report is written.
/// - Exit status 1 when serving, request dispatch, or report writing fails.
///
/// Transformation:
/// - Starts a local `terlc serve` process for a source-backed dynamic handler
///   and measures HTTP round trips through the VM handler lane.
fn run_vm_http_runtime_baseline_cli() -> ExitCode {
    let options = VmHttpBenchmarkOptions::from_env();
    let report = match run_vm_http_runtime_baseline(&options) {
        Ok(report) => report,
        Err(error) => VmHttpBenchmarkReport::failed(&options, error),
    };
    if let Err(error) = write_report(&options.output, &report) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match report.status {
        BenchmarkStatus::Completed => {
            println!(
                "[vm-http-runtime-baseline] completed; wrote {}",
                options.output.display()
            );
            ExitCode::SUCCESS
        }
        BenchmarkStatus::Failed => {
            eprintln!(
                "[vm-http-runtime-baseline] failed: {}; wrote {}",
                report.error_reason.as_deref().unwrap_or("unknown error"),
                options.output.display()
            );
            ExitCode::from(1)
        }
        BenchmarkStatus::Skipped => {
            println!(
                "[vm-http-runtime-baseline] skipped: {}; wrote {}",
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

/// VM HTTP benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VmHttpBenchmarkOptions {
    output: PathBuf,
    iterations: usize,
    compiler_binary: Option<PathBuf>,
}

impl VmHttpBenchmarkOptions {
    /// Reads VM HTTP benchmark options from the process environment.
    ///
    /// Inputs:
    /// - `TERLAN_BENCH_VM_HTTP_OUTPUT`.
    /// - `TERLAN_BENCH_VM_HTTP_ITERATIONS`.
    /// - `TERLAN_BENCH_TERLC_BIN`.
    ///
    /// Output:
    /// - Complete VM HTTP benchmark options.
    ///
    /// Transformation:
    /// - Uses low default iterations because the lane still exercises a full
    ///   loopback server/client path even though handlers are cached at serve
    ///   startup.
    fn from_env() -> Self {
        Self {
            output: env::var_os("TERLAN_BENCH_VM_HTTP_OUTPUT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_VM_HTTP_OUTPUT)),
            iterations: read_usize_var(
                "TERLAN_BENCH_VM_HTTP_ITERATIONS",
                DEFAULT_VM_HTTP_ITERATIONS,
            ),
            compiler_binary: env::var_os("TERLAN_BENCH_TERLC_BIN").map(PathBuf::from),
        }
    }
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

/// VM HTTP runtime benchmark report serialized to JSON.
#[derive(Debug, Clone, Serialize)]
struct VmHttpBenchmarkReport {
    benchmark: &'static str,
    status: BenchmarkStatus,
    timestamp_unix_seconds: u64,
    terlan_version: &'static str,
    rustc_version: Option<String>,
    runtime_stack: VmHttpRuntimeStack,
    iterations: usize,
    measurements: Vec<Measurement>,
    assertions: Vec<AssertionResult>,
    skip_reason: Option<String>,
    error_reason: Option<String>,
}

impl VmHttpBenchmarkReport {
    /// Builds a completed VM HTTP benchmark report.
    fn completed(
        options: &VmHttpBenchmarkOptions,
        runtime_stack: VmHttpRuntimeStack,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
    ) -> Self {
        Self {
            benchmark: VM_HTTP_COMMAND,
            status: BenchmarkStatus::Completed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            runtime_stack,
            iterations: options.iterations,
            measurements,
            assertions,
            skip_reason: None,
            error_reason: None,
        }
    }

    /// Builds a failed VM HTTP benchmark report.
    fn failed(options: &VmHttpBenchmarkOptions, reason: impl Into<String>) -> Self {
        Self {
            benchmark: VM_HTTP_COMMAND,
            status: BenchmarkStatus::Failed,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            runtime_stack: VmHttpRuntimeStack::unresolved(),
            iterations: options.iterations,
            measurements: Vec::new(),
            assertions: Vec::new(),
            skip_reason: None,
            error_reason: Some(reason.into()),
        }
    }
}

impl VmBenchmarkReport {
    /// Builds a completed VM benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `runtime_stack`: resolved binary/runtime metadata.
    /// - `measurements`: completed timing tracks.
    /// - `assertions`: correctness assertions for completed tracks.
    /// - `skipped_tracks`: required future tracks with stable skip reasons.
    ///
    /// Output:
    /// - Serializable completed VM baseline report.
    ///
    /// Transformation:
    /// - Records measured tracks and any deliberate future VM-owned tracks so
    ///   the report stays useful as a migration baseline.
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

/// VM HTTP runtime stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
struct VmHttpRuntimeStack {
    compiler_binary: String,
    server_runtime: &'static str,
    handler_runtime: &'static str,
    client_runtime: &'static str,
    handler_loading: &'static str,
}

impl VmHttpRuntimeStack {
    /// Builds resolved VM HTTP runtime metadata.
    fn resolved(compiler_binary: &Path) -> Self {
        Self {
            compiler_binary: compiler_binary.display().to_string(),
            server_runtime: "terlc serve over legacy host transport",
            handler_runtime: "Terlan VM dynamic handler dispatch",
            client_runtime: "Hyper client over loopback HTTP",
            handler_loading: "source-metadata invalidated cache of loaded VM modules",
        }
    }

    /// Builds unresolved VM HTTP runtime metadata for failed reports.
    fn unresolved() -> Self {
        Self {
            compiler_binary: "<unresolved>".to_string(),
            server_runtime: "terlc serve over legacy host transport",
            handler_runtime: "Terlan VM dynamic handler dispatch",
            client_runtime: "Hyper client over loopback HTTP",
            handler_loading: "source-metadata invalidated cache of loaded VM modules",
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
            artifact_execution: "terlc build --target terlan-vm; terlan-vm load <application.tvm>",
            skipped_track_policy:
                "no required skipped VM-owned tracks; future skips must be explicit",
        }
    }

    /// Builds unresolved VM runtime metadata for failed reports.
    fn unresolved() -> Self {
        Self {
            vm_binary: "<unresolved>".to_string(),
            compiler_binary: "<unresolved>".to_string(),
            source_execution: "terlan-vm run <source.terl> --test-eval",
            artifact_execution: "terlc build --target terlan-vm; terlan-vm load <application.tvm>",
            skipped_track_policy:
                "no required skipped VM-owned tracks; future skips must be explicit",
        }
    }
}

/// VM benchmark track that is required but not executable yet.
#[derive(Debug, Clone, Copy, Serialize)]
struct SkippedTrack {
    name: &'static str,
    reason: &'static str,
    detail: &'static str,
}

/// VM benchmark tracks that must remain visible until promoted to measurements.
///
/// This list is intentionally empty for the current release line: every
/// previously required skipped VM performance lane has been promoted into a
/// measured benchmark. Future skipped lanes must be added here deliberately so
/// they remain visible in the baseline report.
const REQUIRED_VM_SKIPPED_TRACKS: &[SkippedTrack] = &[];
