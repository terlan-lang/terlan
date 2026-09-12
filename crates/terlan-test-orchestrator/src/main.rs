#![forbid(unsafe_code)]

use phase_plan::test_phases;
#[cfg(test)]
use phase_plan::{terlan_library_phase, workspace_doctest_phase};
use std::env;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

mod cargo_artifact_stream;
mod cargo_compiler_environment;
mod cargo_default_tools;
mod cargo_harness_admission;
mod cargo_metadata_owner;
mod cargo_proxy_path;
mod cargo_target_declaration;
mod cargo_tool_settings;
mod compiler_invocation;
mod configured_cargo_tools;
mod coverage_batch;
mod coverage_bridge;
mod coverage_requests;
mod coverage_resolution;
mod doctest_output;
mod driver_snapshot;
mod executable_binding;
mod execution_environment;
mod file_identity;
mod hosted_checkpoint;
mod hosted_coverage;
mod hosted_make;
mod hosted_producer;
mod launch_ledger;
mod library_selection;
mod make_coverage;
mod make_environment;
mod native_coverage;
mod owned_command;
mod phase_plan;
mod report_file;
mod rust_toolchain;
mod rustdoc_owner;
mod rustup_selection;
mod selected_compiler;
mod shutdown;
mod source_inventory;
mod suite_inputs;
mod test_execution;
mod test_inventory;
mod test_result_log;
mod test_selections;
mod tool_configuration;
mod tool_tree;
mod validation_inputs;
mod verify_coverage;
mod workspace_native;
mod workspace_native_runner;
mod workspace_native_storage;

#[cfg(test)]
#[path = "main_test.rs"]
#[cfg(test)]
mod test_orchestrator_test;

#[cfg(test)]
#[path = "cargo_runner_test.rs"]
mod cargo_runner_test;

// Library tests exercise process-global compiler paths and environment
// contracts. Parallel libtest threads can make one fixture execute another
// fixture's compiler, so the canonical evidence run is serial by default.
const DEFAULT_TEST_THREADS: usize = 1;
const DEFAULT_PHASE_TIMEOUT_SECONDS: u64 = 1_800;
const RELEASE_COVERAGE_OWNS_TERLC_ENV: &str = "TERLAN_RELEASE_COVERAGE_OWNS_TERLC_TESTS";
const VALIDATION_FEATURES: &str = "quality-tools,editor-lsp,benchmark-tools";
const REPORT_PATH_ENV: &str = "TERLAN_RUST_SUITE_REPORT";
const PHASE_TIMEOUT_ENV: &str = "TERLAN_TEST_PHASE_TIMEOUT_SECONDS";
const TIER_INVENTORY_PATH: &str = "docs/quality/RUST_VALIDATION_TIERS.tsv";
const TIER_INVENTORY: &str = include_str!("../../../docs/quality/RUST_VALIDATION_TIERS.tsv");
const INTEGRATION_FILTERS: [&str; 4] = ["quality::", "lsp::", "benchmark::", "comprehension"];
const MAX_CARGO_PHASES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationTier {
    FastUnit,
    Integration,
    AotNativeLink,
    ConcurrencyTimeout,
    Performance,
    ControlledHost,
}

impl ValidationTier {
    const ALL: [Self; 6] = [
        Self::FastUnit,
        Self::Integration,
        Self::AotNativeLink,
        Self::ConcurrencyTimeout,
        Self::Performance,
        Self::ControlledHost,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::FastUnit => "fast-unit",
            Self::Integration => "integration",
            Self::AotNativeLink => "aot-native-link",
            Self::ConcurrencyTimeout => "concurrency-timeout",
            Self::Performance => "performance",
            Self::ControlledHost => "controlled-host",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestPhase {
    name: &'static str,
    tier: ValidationTier,
    executor: PhaseExecutor,
    args: Vec<&'static str>,
    environment: Vec<(&'static str, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseExecutor {
    Cargo,
    CargoNative,
    TerlanHarness,
}

impl PhaseExecutor {
    const fn is_cargo(self) -> bool {
        matches!(self, Self::Cargo | Self::CargoNative)
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::CargoNative => "cargo-native-harnesses",
            Self::TerlanHarness => "direct-terlan-harness",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhaseResult {
    name: &'static str,
    tier: ValidationTier,
    executor: &'static str,
    wall_time_ms: u128,
    outcome: &'static str,
    child_pid: Option<u32>,
    test_execution: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExternalTierOwner {
    tier: ValidationTier,
    make_target: &'static str,
    isolation: &'static str,
}

const EXTERNAL_TIER_OWNERS: [ExternalTierOwner; 3] = [
    ExternalTierOwner {
        tier: ValidationTier::ConcurrencyTimeout,
        make_target: "vm-multicore-memory-model-check",
        isolation: "serial-bounded-watchdog",
    },
    ExternalTierOwner {
        tier: ValidationTier::Performance,
        make_target: "vm-multicore-performance-record",
        isolation: "manual-observation",
    },
    ExternalTierOwner {
        tier: ValidationTier::ControlledHost,
        make_target: "native-boundary-postgres-docker-check",
        isolation: "declared-docker-host",
    },
];

fn main() -> ExitCode {
    let _driver_lease = match driver_snapshot::running_lease() {
        Ok(lease) => lease,
        Err(error) => {
            eprintln!("[rust-driver] {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(program) = env::args_os()
        .next()
        .filter(|program| rustdoc_owner::is_observer(program))
    {
        return rustdoc_owner::main(program, env::args_os().skip(1));
    }
    let mut arguments = env::args_os().skip(1);
    match arguments.next().as_deref() {
        Some(value) if value == std::ffi::OsStr::new("--run-owned") => {
            return owned_command::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--run-library-tests") => {
            return library_selection::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--install-snapshot") => {
            return driver_snapshot::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--cargo-native-runner") => {
            return workspace_native_runner::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--cargo-metadata") => {
            return cargo_metadata_owner::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--inspect-coverage") => {
            return test_selections::inspect_main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--verify-coverage") => {
            return verify_coverage::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--verify-cargo-coverage") => {
            return coverage_batch::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--check-hosted-coverage-records") => {
            return hosted_coverage::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--with-cargo-coverage") => {
            return make_coverage::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--with-hosted-cargo-coverage") => {
            return hosted_make::main(arguments);
        }
        Some(value) if value == std::ffi::OsStr::new("--coverage-request") => {
            return make_coverage::request(arguments);
        }
        Some(value) => {
            eprintln!("[rust-test-suite] unknown command {value:?}; use no arguments to run the suite, --cargo-metadata, --inspect-coverage or --verify-coverage REPORT -- [LIBTEST SELECTORS]");
            return ExitCode::from(2);
        }
        None => {}
    }
    if let Err(error) = validate_tier_inventory() {
        eprintln!("[rust-test-suite] invalid tier inventory: {error}");
        return ExitCode::from(1);
    }
    let environment = match execution_environment::ExecutionEnvironment::capture() {
        Ok(environment) => environment,
        Err(error) => {
            eprintln!("[rust-test-suite] {}: {}", error.outcome, error.detail);
            return ExitCode::from(1);
        }
    };
    let test_threads = environment
        .value("TERLAN_TEST_THREADS")
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or(DEFAULT_TEST_THREADS);
    let timeout = phase_timeout(&environment);
    let shutdown = match shutdown::Shutdown::install() {
        Ok(shutdown) => shutdown,
        Err(error) => {
            eprintln!("[rust-test-suite] cannot install cancellation: {error}");
            return ExitCode::from(1);
        }
    };
    let control = ProcessControl::new(timeout).with_cancellation(shutdown.flag());
    let coverage_owns_terlc = environment
        .value(RELEASE_COVERAGE_OWNS_TERLC_ENV)
        .as_deref()
        == Some(std::ffi::OsStr::new("1"));
    let phases = test_phases(coverage_owns_terlc);
    if phases
        .iter()
        .filter(|phase| phase.executor.is_cargo())
        .count()
        + 1
        > MAX_CARGO_PHASES
    {
        eprintln!("[rust-test-suite] planned Cargo phases exceed the launch budget");
        return ExitCode::from(1);
    }
    let mut ledger =
        match launch_ledger::LaunchLedger::new(&report_path(&environment), test_threads, timeout) {
            Ok(ledger) => ledger,
            Err(error) => {
                eprintln!("[rust-test-suite] cannot own launch inventory: {error}");
                return ExitCode::from(1);
            }
        };
    let started = Instant::now();
    if let Err(error) = suite_inputs::admit(&mut ledger, &environment, &phases, control) {
        eprintln!(
            "[rust-test-suite] input admission {}: {}",
            error.outcome, error.detail
        );
        return ExitCode::FAILURE;
    }
    let executable_inputs = ledger.executables().clone();
    let metadata_packages = match ledger.metadata() {
        Ok(metadata) => metadata.packages(),
        Err(error) => {
            eprintln!("[rust-test-suite] {}", error.detail);
            return ExitCode::FAILURE;
        }
    };
    let rustdoc_inputs = match ledger.rustdoc_inputs(control) {
        Ok(inputs) => inputs,
        Err(error) => {
            eprintln!("[rust-test-suite] Rustdoc admission: {}", error.detail);
            return ExitCode::FAILURE;
        }
    };
    if coverage_owns_terlc {
        println!("[rust-test-suite] release coverage owns the normal Terlan library phase");
    }
    println!("[rust-test-suite] building union-feature Terlan harness once");
    let harness = match ledger.execute(
        "Terlan union-feature harness build",
        ValidationTier::FastUnit,
        "cargo-build",
        |launched| {
            prepare_terlan_harness(
                &executable_inputs.verify_program("cargo", control)?,
                &environment,
                control,
                launched,
            )
        },
    ) {
        Ok(harness) => harness,
        Err(error) => {
            eprintln!(
                "[rust-test-suite] harness preparation {}: {}",
                error.outcome, error.detail
            );
            return ExitCode::from(1);
        }
    };
    if let Err(error) =
        cargo_metadata_owner::Handoff::admit_harness(&metadata_packages, &harness.json())
            .and_then(|()| ledger.bind_test_programs(control))
            .and_then(|()| ledger.bind_declared_harness(harness, control))
    {
        eprintln!(
            "[rust-test-suite] harness identity {}: {}",
            error.outcome, error.detail
        );
        return ExitCode::from(1);
    }
    let executable_inputs = ledger.executables().clone();
    let selections = match test_inventory::prepare(
        &phases,
        coverage_owns_terlc,
        control,
        &executable_inputs,
        &environment,
        &mut ledger,
    ) {
        Ok(selections) => selections,
        Err(error) => {
            eprintln!(
                "[rust-test-suite] test inventory {}: {}",
                error.outcome, error.detail
            );
            return ExitCode::from(1);
        }
    };
    if let Err(error) = ledger.admit_test_selections(&phases, &selections, control) {
        eprintln!(
            "[rust-test-suite] test selection admission: {}",
            error.detail
        );
        return ExitCode::FAILURE;
    }
    for phase in phases {
        if phase.executor == PhaseExecutor::Cargo && rustdoc_inputs.targets.is_empty() {
            continue;
        }
        println!("[rust-test-suite] running {}", phase.name);
        let phase_started = Instant::now();
        if let Err(error) = ledger.execute_test(
            phase.name,
            phase.tier,
            phase.executor.as_str(),
            |launched, partial| {
                let executable = match phase.executor {
                    PhaseExecutor::Cargo | PhaseExecutor::CargoNative => {
                        executable_inputs.verify_program("cargo", control)?
                    }
                    PhaseExecutor::TerlanHarness => executable_inputs.verify_harness(control)?,
                };
                if phase.executor == PhaseExecutor::CargoNative {
                    let runner =
                        executable_inputs.verify_program("terlan-test-orchestrator", control)?;
                    return workspace_native::run(
                        &phase,
                        (
                            &executable,
                            &runner,
                            &metadata_packages,
                            &executable_inputs.json()["declaration"],
                        ),
                        &environment,
                        test_threads,
                        control,
                        launched,
                        partial,
                    );
                }
                if phase.executor == PhaseExecutor::Cargo {
                    let runner =
                        executable_inputs.verify_program("terlan-test-orchestrator", control)?;
                    return rustdoc_owner::run(
                        &phase,
                        (&executable, &runner, &rustdoc_inputs),
                        &environment,
                        test_threads,
                        control,
                        launched,
                        partial,
                    );
                }
                test_execution::run(
                    &phase,
                    &executable,
                    &environment,
                    test_threads,
                    selections.selections.get(phase.name).cloned(),
                    control,
                    launched,
                )
            },
        ) {
            eprintln!(
                "[rust-test-suite] {} {}: {}",
                phase.name, error.outcome, error.detail
            );
            return ExitCode::from(1);
        }
        println!(
            "[rust-test-suite] {} passed in {:.2}s",
            phase.name,
            phase_started.elapsed().as_secs_f64()
        );
    }
    if let Err(error) = ledger.verify_test_selections(control) {
        eprintln!(
            "[rust-test-suite] test selection closeout: {}",
            error.detail
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = suite_inputs::close(&mut ledger, &environment, control) {
        eprintln!(
            "[rust-test-suite] input closeout {}: {}",
            error.outcome, error.detail
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = ledger.finish_unless_cancelled(shutdown.flag()) {
        eprintln!("[rust-test-suite] cannot seal launch inventory: {error}");
        return ExitCode::from(1);
    }
    println!(
        "[rust-test-suite] all owned harnesses passed in {:.2}s",
        started.elapsed().as_secs_f64()
    );
    ExitCode::SUCCESS
}

#[derive(Debug)]
struct PhaseFailure {
    outcome: &'static str,
    detail: String,
}

fn prepare_terlan_harness(
    cargo: &Path,
    environment: &execution_environment::ExecutionEnvironment,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<cargo_harness_admission::DeclaredHarness, PhaseFailure> {
    let mut command = environment.test_command(cargo);
    command
        .args(phase_plan::workspace_native_arguments())
        .arg("--no-run");
    let root = home::env::Env::current_dir(environment).map_err(|error| PhaseFailure {
        outcome: "harness-declaration-failed",
        detail: error.to_string(),
    })?;
    let mut artifacts = cargo_artifact_stream::CargoArtifactStream::terlan_library(&root)?;
    artifacts.capture(&mut command, control, launched, |_| Ok(()))?;
    artifacts.finish_terlan_library()
}

#[cfg(test)]
fn run_closed_command(command: &mut Command, timeout: Duration) -> Result<(), PhaseFailure> {
    terlan_process_owner::run(command, timeout).map_err(process_failure)
}

#[cfg(test)]
fn run_closed_command_captured(
    command: &mut Command,
    timeout: Duration,
) -> Result<Vec<u8>, PhaseFailure> {
    terlan_process_owner::capture_stdout(command, timeout, 16 * 1024 * 1024)
        .map_err(process_failure)
}

fn process_failure(error: terlan_process_owner::Failure) -> PhaseFailure {
    PhaseFailure {
        outcome: error.kind,
        detail: error.detail,
    }
}

fn phase_timeout(environment: &execution_environment::ExecutionEnvironment) -> Duration {
    let configured = environment
        .value(PHASE_TIMEOUT_ENV)
        .and_then(|value| value.into_string().ok());
    phase_timeout_from(configured.as_deref())
}

fn phase_timeout_from(configured: Option<&str>) -> Duration {
    let seconds = configured
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_PHASE_TIMEOUT_SECONDS);
    Duration::from_secs(seconds)
}

fn report_path(environment: &execution_environment::ExecutionEnvironment) -> PathBuf {
    environment
        .value(REPORT_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/rust-test-suite-report.json"))
}

fn write_report(
    file: &mut report_file::ReportFile,
    decision: &str,
    test_threads: usize,
    phase_timeout: Duration,
    wall_time: Duration,
    results: &[PhaseResult],
    inputs: &validation_inputs::ValidationInputs,
) -> Result<(), String> {
    let tier_inventory_rows = validate_tier_inventory()?;
    let phases = results
        .iter()
        .map(|result| {
            format!(
                "    {{\"name\":\"{}\",\"tier\":\"{}\",\"executor\":\"{}\",\"outcome\":\"{}\",\"wall_time_ms\":{},\"child_pid\":{},\"test_execution\":{}}}",
                result.name,
                result.tier.as_str(),
                result.executor,
                result.outcome,
                result.wall_time_ms,
                serde_json::json!(result.child_pid),
                serde_json::json!(result.test_execution),
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let tiers = ValidationTier::ALL
        .iter()
        .map(|tier| format!("    \"{}\"", tier.as_str()))
        .collect::<Vec<_>>()
        .join(",\n");
    let external_owners = EXTERNAL_TIER_OWNERS
        .iter()
        .map(|owner| {
            format!(
                "    {{\"tier\":\"{}\",\"owner\":\"{}\",\"isolation\":\"{}\"}}",
                owner.tier.as_str(),
                owner.make_target,
                owner.isolation
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let report = format!(
        concat!(
            "{{\n",
            "  \"schema\": \"terlan.rust-test-suite.v4\",\n",
            "  \"run_id\": \"{}\",\n",
            "  \"decision\": \"{}\",\n",
            "  \"launch_accounting_scope\": \"orchestrator-direct-children\",\n",
            "  \"source_binding\": {},\n",
            "  \"executable_binding\": {},\n",
            "  \"environment_binding\": {},\n",
            "  \"tool_configuration_binding\": {},\n",
            "  \"cargo_tool_binding\": {},\n",
            "  \"rust_toolchain_binding\": {},\n",
            "  \"selected_compiler_binding\": {},\n",
            "  \"cargo_metadata_binding\": {},\n",
            "  \"test_selection_binding\": {},\n",
            "  \"closed_stdin\": true,\n",
            "  \"test_threads\": {},\n",
            "  \"phase_timeout_seconds\": {},\n",
            "  \"direct_cargo_launch_count\": {},\n",
            "  \"direct_cargo_launch_maximum\": {},\n",
            "  \"direct_process_launch_count\": {},\n",
            "  \"wall_time_ms\": {},\n",
            "  \"tier_inventory_path\": \"{}\",\n",
            "  \"tier_inventory_row_count\": {},\n",
            "  \"tier_inventory\": [\n{}\n  ],\n",
            "  \"external_tier_owners\": [\n{}\n  ],\n",
            "  \"phases\": [\n{}\n  ]\n",
            "}}\n"
        ),
        file.run_id(),
        decision,
        inputs.source.json(),
        inputs.executables.json(),
        inputs.environment,
        inputs.configuration.json(),
        inputs.cargo_tools.json(),
        inputs.toolchain.json(),
        inputs.compiler.json(),
        inputs
            .metadata
            .as_ref()
            .map(cargo_metadata_owner::Handoff::json)
            .unwrap_or(serde_json::Value::Null),
        inputs
            .test_selections
            .as_ref()
            .map(test_selections::TestSelections::json)
            .unwrap_or(serde_json::Value::Null),
        test_threads,
        phase_timeout.as_secs(),
        results
            .iter()
            .filter(|result| result.child_pid.is_some() && result.executor.starts_with("cargo"))
            .count(),
        MAX_CARGO_PHASES,
        results
            .iter()
            .filter(|result| result.child_pid.is_some())
            .count(),
        wall_time.as_millis(),
        TIER_INVENTORY_PATH,
        tier_inventory_rows,
        tiers,
        external_owners,
        phases
    );
    file.publish(report.as_bytes())
}

fn validate_tier_inventory() -> Result<usize, String> {
    let mut lines = TIER_INVENTORY.lines();
    if lines.next() != Some("selector\ttier\towner\tisolation") {
        return Err("header must declare selector, tier, owner, and isolation".to_string());
    }
    let mut selectors = Vec::new();
    let mut represented = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) {
            return Err(format!(
                "row {} is not a complete four-field record",
                index + 2
            ));
        }
        if selectors.contains(&fields[0]) {
            return Err(format!("selector `{}` has more than one tier", fields[0]));
        }
        let Some(tier) = ValidationTier::ALL
            .iter()
            .copied()
            .find(|tier| tier.as_str() == fields[1])
        else {
            return Err(format!(
                "selector `{}` has unknown tier `{}`",
                fields[0], fields[1]
            ));
        };
        selectors.push(fields[0]);
        if !represented.contains(&tier) {
            represented.push(tier);
        }
    }
    if represented.len() != ValidationTier::ALL.len() {
        return Err("inventory does not represent all six validation tiers".to_string());
    }
    Ok(selectors.len())
}

#[cfg(test)]
fn cargo_program() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn native_worker_path() -> String {
    env::current_dir()
        .unwrap_or_default()
        .join(prebuilt_binary("terlan-native-worker"))
        .to_string_lossy()
        .into_owned()
}

fn prebuilt_binary(name: &str) -> PathBuf {
    PathBuf::from("target/debug").join(format!("{name}{}", env::consts::EXE_SUFFIX))
}

fn executable_paths(
    environment: &execution_environment::ExecutionEnvironment,
) -> Result<Vec<(&'static str, PathBuf)>, PhaseFailure> {
    let path = environment.value("PATH").unwrap_or_default();
    let cargo = environment.value("CARGO").unwrap_or_else(|| "cargo".into());
    let cargo = cargo.to_str().ok_or_else(|| PhaseFailure {
        outcome: "executable-identity-failed",
        detail: "selected Cargo path is not UTF-8".into(),
    })?;
    Ok(vec![
        ("git", executable_binding::resolve_program("git", &path)?),
        (
            "rustup",
            executable_binding::resolve_program("rustup", environment.test_path())?,
        ),
        (
            "cargo",
            executable_binding::resolve_program(cargo, environment.test_path())?,
        ),
        (
            "terlan-test-orchestrator",
            env::current_exe().map_err(|error| PhaseFailure {
                outcome: "executable-identity-failed",
                detail: error.to_string(),
            })?,
        ),
    ])
}

fn test_program_paths() -> Vec<(&'static str, PathBuf)> {
    ["terlc", "terlan-vm", "terlan-native-worker"]
        .into_iter()
        .map(|role| (role, prebuilt_binary(role)))
        .collect()
}
