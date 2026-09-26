use super::*;

pub(super) const POSTGRES_COMMAND: &str = "native-boundary-postgres-baseline";
pub(super) const VM_COMMAND: &str = "vm-performance-baseline";
pub(super) const DEFAULT_OUTPUT: &str =
    "../benchmarks/results/native-boundary-postgres-baseline.latest.json";
pub(super) const DEFAULT_VM_OUTPUT: &str =
    "../benchmarks/results/vm-performance-baseline.latest.json";
pub(super) const DEFAULT_ITERATIONS: usize = 100;
pub(super) const DEFAULT_VM_ITERATIONS: usize = 3;
pub(super) const DEFAULT_CONCURRENCY: usize = 8;
pub(super) const MAP_BENCHMARK_SIZES: &[usize] = &[16, 32, 33, 127, 128, 129, 5_000];
pub(super) const MAP_STRESS_SIZE: usize = 5_000;
pub(super) const COLLISION_HEAVY_MAP_SIZE: usize = 512;

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
pub(super) fn run() -> ExitCode {
    run_command(env::args().nth(1).as_deref())
}

fn run_command(command: Option<&str>) -> ExitCode {
    match command {
        Some(POSTGRES_COMMAND) => run_postgres_baseline_cli(),
        Some(VM_COMMAND) => run_vm_performance_baseline_cli(),
        Some(binary_protocol::COMMAND) => binary_protocol::run_cli(),
        Some(persistent_actor::COMMAND) => persistent_actor::run_cli(),
        Some(runtime_workloads::COMMAND) => runtime_workloads::run_cli(),
        Some(http_aot_performance::COMMAND) => http_aot_performance::run_cli(),
        Some(http_aot_performance::COMPARE_COMMAND) => http_aot_performance::run_compare_cli(),
        Some(http_aot_performance::SELF_TEST_COMMAND) => http_aot_performance::run_self_test_cli(),
        Some(command) => {
            eprintln!("unsupported terlan-benchmark command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!(
                "usage: terlan-benchmark <{POSTGRES_COMMAND}|{VM_COMMAND}|{}|{}|{}|{}|{}|{}>",
                binary_protocol::COMMAND,
                persistent_actor::COMMAND,
                runtime_workloads::COMMAND,
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
///
/// Output:
/// - Exit status 0 when a completed or skipped report is written.
/// - Exit status 1 when benchmark execution or report writing fails.
///
/// Transformation:
/// - Builds benchmark options, records the report, writes JSON, and prints a
///   stable one-line status for Make/CI logs.
pub(super) fn run_postgres_baseline_cli() -> ExitCode {
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
pub(super) fn run_vm_performance_baseline_cli() -> ExitCode {
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
pub(super) struct BenchmarkOptions {
    pub(super) output: PathBuf,
    pub(super) postgres_url: Option<String>,
    pub(super) postgres_url_source: Option<&'static str>,
    pub(super) iterations: usize,
    pub(super) concurrency: usize,
}

/// VM benchmark configuration derived from the environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmBenchmarkOptions {
    pub(super) output: PathBuf,
    pub(super) iterations: usize,
    pub(super) vm_binary: Option<PathBuf>,
    pub(super) compiler_binary: Option<PathBuf>,
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
    pub(super) fn from_env() -> Self {
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
    pub(super) fn from_env() -> Self {
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
pub(super) fn read_url_var(name: &str) -> Option<String> {
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
pub(super) struct BenchmarkReport {
    pub(super) benchmark: &'static str,
    pub(super) status: BenchmarkStatus,
    pub(super) timestamp_unix_seconds: u64,
    pub(super) terlan_version: &'static str,
    pub(super) rustc_version: Option<String>,
    pub(super) adapter_stack: AdapterStack,
    pub(super) postgres_url_source: Option<&'static str>,
    pub(super) postgres_url_redacted: Option<String>,
    pub(super) iterations: usize,
    pub(super) concurrency: usize,
    pub(super) measurements: Vec<Measurement>,
    pub(super) assertions: Vec<AssertionResult>,
    pub(super) skip_reason: Option<String>,
    pub(super) error_reason: Option<String>,
}

/// VM performance baseline report serialized to JSON.
#[derive(Debug, Clone, Serialize)]
pub(super) struct VmBenchmarkReport {
    pub(super) benchmark: &'static str,
    pub(super) status: BenchmarkStatus,
    pub(super) timestamp_unix_seconds: u64,
    pub(super) terlan_version: &'static str,
    pub(super) rustc_version: Option<String>,
    pub(super) runtime_stack: VmRuntimeStack,
    pub(super) iterations: usize,
    pub(super) measurements: Vec<Measurement>,
    pub(super) assertions: Vec<AssertionResult>,
    pub(super) skip_reason: Option<String>,
    pub(super) error_reason: Option<String>,
}

impl VmBenchmarkReport {
    /// Builds a completed VM benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark options.
    /// - `runtime_stack`: resolved binary/runtime metadata.
    /// - `measurements`: completed timing tracks.
    /// - `assertions`: correctness assertions for completed tracks.
    ///
    /// Output:
    /// - Serializable completed VM baseline report.
    ///
    /// Transformation:
    /// - Records measured tracks and their correctness assertions.
    pub(super) fn completed(
        options: &VmBenchmarkOptions,
        runtime_stack: VmRuntimeStack,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
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
    pub(super) fn failed(options: &VmBenchmarkOptions, reason: impl Into<String>) -> Self {
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
            skip_reason: None,
            error_reason: Some(reason.into()),
        }
    }
}

/// VM runtime stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
pub(super) struct VmRuntimeStack {
    pub(super) vm_binary: String,
    pub(super) compiler_binary: String,
    pub(super) artifact_execution: &'static str,
}

impl VmRuntimeStack {
    /// Builds resolved VM runtime metadata.
    pub(super) fn resolved(vm_binary: &Path, compiler_binary: &Path) -> Self {
        Self {
            vm_binary: vm_binary.display().to_string(),
            compiler_binary: compiler_binary.display().to_string(),
            artifact_execution: "terlc build --target terlan-vm; terlan-vm load <application.tvm>",
        }
    }

    /// Builds unresolved VM runtime metadata for failed reports.
    pub(super) fn unresolved() -> Self {
        Self {
            vm_binary: "<unresolved>".to_string(),
            compiler_binary: "<unresolved>".to_string(),
            artifact_execution: "terlc build --target terlan-vm; terlan-vm load <application.tvm>",
        }
    }
}

#[cfg(test)]
#[path = "cli_test.rs"]
mod tests;
