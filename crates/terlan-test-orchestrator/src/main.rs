#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "main_test.rs"]
#[cfg(test)]
mod test_orchestrator_test;

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
const MAX_CARGO_PHASES: usize = 2;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseExecutor {
    Cargo,
    TerlanHarness,
}

impl PhaseExecutor {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
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
        make_target: "vm-multicore-performance-check",
        isolation: "dedicated-controlled-host",
    },
    ExternalTierOwner {
        tier: ValidationTier::ControlledHost,
        make_target: "native-boundary-postgres-docker-check",
        isolation: "declared-docker-host",
    },
];

fn main() -> ExitCode {
    if let Err(error) = validate_tier_inventory() {
        eprintln!("[rust-test-suite] invalid tier inventory: {error}");
        return ExitCode::from(1);
    }
    let test_threads = env::var("TERLAN_TEST_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or(DEFAULT_TEST_THREADS);
    let started = Instant::now();
    let phase_timeout = phase_timeout();
    let coverage_owns_terlc = env::var(RELEASE_COVERAGE_OWNS_TERLC_ENV).as_deref() == Ok("1");
    let phases = test_phases(coverage_owns_terlc);
    let cargo_phase_count = phases
        .iter()
        .filter(|phase| phase.executor == PhaseExecutor::Cargo)
        .count()
        + 1;
    if cargo_phase_count > MAX_CARGO_PHASES {
        eprintln!(
            "[rust-test-suite] {} Cargo phases exceed the {}-phase budget",
            cargo_phase_count, MAX_CARGO_PHASES
        );
        return ExitCode::from(1);
    }
    let mut results = Vec::with_capacity(phases.len() + 1);

    if coverage_owns_terlc {
        println!(
            "[rust-test-suite] release coverage owns the complete normal Terlan library test phase"
        );
    }

    let harness_started = Instant::now();
    println!("[rust-test-suite] building union-feature Terlan harness once");
    let terlan_harness = match prepare_terlan_harness(phase_timeout) {
        Ok(path) => {
            results.push(PhaseResult {
                name: "Terlan union-feature harness build",
                tier: ValidationTier::FastUnit,
                executor: "cargo-build",
                wall_time_ms: harness_started.elapsed().as_millis(),
                outcome: "passed",
            });
            path
        }
        Err(error) => {
            results.push(PhaseResult {
                name: "Terlan union-feature harness build",
                tier: ValidationTier::FastUnit,
                executor: "cargo-build",
                wall_time_ms: harness_started.elapsed().as_millis(),
                outcome: error.outcome,
            });
            eprintln!(
                "[rust-test-suite] union-feature harness build {}: {}",
                error.outcome, error.detail
            );
            if let Err(report_error) = write_report(
                &report_path(),
                "fail",
                test_threads,
                phase_timeout,
                started.elapsed(),
                &results,
            ) {
                eprintln!("[rust-test-suite] cannot write failure report: {report_error}");
            }
            return ExitCode::from(1);
        }
    };

    for phase in phases {
        let phase_started = Instant::now();
        println!("[rust-test-suite] running {}", phase.name);
        let outcome = run_phase(&phase, &terlan_harness, test_threads, phase_timeout);
        let wall_time = phase_started.elapsed();
        match outcome {
            Ok(()) => {
                results.push(PhaseResult {
                    name: phase.name,
                    tier: phase.tier,
                    executor: phase.executor.as_str(),
                    wall_time_ms: wall_time.as_millis(),
                    outcome: "passed",
                });
                println!(
                    "[rust-test-suite] {} passed in {:.2}s",
                    phase.name,
                    wall_time.as_secs_f64()
                );
            }
            Err(error) => {
                results.push(PhaseResult {
                    name: phase.name,
                    tier: phase.tier,
                    executor: phase.executor.as_str(),
                    wall_time_ms: wall_time.as_millis(),
                    outcome: error.outcome,
                });
                eprintln!(
                    "[rust-test-suite] {} {}: {}",
                    phase.name, error.outcome, error.detail
                );
                if let Err(report_error) = write_report(
                    &report_path(),
                    "fail",
                    test_threads,
                    phase_timeout,
                    started.elapsed(),
                    &results,
                ) {
                    eprintln!("[rust-test-suite] cannot write failure report: {report_error}");
                }
                return ExitCode::from(1);
            }
        }
    }

    if let Err(error) = write_report(
        &report_path(),
        "pass",
        test_threads,
        phase_timeout,
        started.elapsed(),
        &results,
    ) {
        eprintln!("[rust-test-suite] cannot write validation report: {error}");
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

fn run_phase(
    phase: &TestPhase,
    terlan_harness: &Path,
    test_threads: usize,
    timeout: Duration,
) -> Result<(), PhaseFailure> {
    let mut command = match phase.executor {
        PhaseExecutor::Cargo => Command::new(cargo_program()),
        PhaseExecutor::TerlanHarness => Command::new(terlan_harness),
    };
    command
        .args(&phase.args)
        .arg("--test-threads")
        .arg(test_threads.to_string())
        .arg("--quiet")
        .env("PATH", test_path());
    run_closed_command(&mut command, timeout)
}

fn prepare_terlan_harness(timeout: Duration) -> Result<PathBuf, PhaseFailure> {
    let mut command = Command::new(cargo_program());
    command
        .args([
            "test",
            "--locked",
            "-p",
            "terlan",
            "--lib",
            "--features",
            VALIDATION_FEATURES,
            "--no-run",
            "--message-format=json",
        ])
        .env("PATH", test_path());
    let output = run_closed_command_captured(&mut command, timeout)?;
    select_terlan_harness(&output)
}

fn select_terlan_harness(output: &[u8]) -> Result<PathBuf, PhaseFailure> {
    let mut executables = output
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == "terlan")
        .filter(|message| {
            message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "lib"))
        })
        .filter(|message| message["profile"]["test"] == true)
        .filter_map(|message| message["executable"].as_str().map(PathBuf::from))
        .collect::<Vec<_>>();
    executables.sort();
    executables.dedup();
    match executables.as_slice() {
        [executable] if executable.is_file() => Ok(executable.clone()),
        [executable] => Err(PhaseFailure {
            outcome: "artifact-missing",
            detail: format!(
                "Cargo selected missing Terlan test harness {}",
                executable.display()
            ),
        }),
        _ => Err(PhaseFailure {
            outcome: "artifact-ambiguous",
            detail: format!(
                "Cargo selected {} Terlan library test harnesses",
                executables.len()
            ),
        }),
    }
}

fn run_closed_command(command: &mut Command, timeout: Duration) -> Result<(), PhaseFailure> {
    let mut child = command
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| PhaseFailure {
            outcome: "launch-failed",
            detail: error.to_string(),
        })?;
    wait_for_child(&mut child, timeout)
}

fn run_closed_command_captured(
    command: &mut Command,
    timeout: Duration,
) -> Result<Vec<u8>, PhaseFailure> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| PhaseFailure {
            outcome: "launch-failed",
            detail: error.to_string(),
        })?;
    let mut stdout = child.stdout.take().ok_or_else(|| PhaseFailure {
        outcome: "capture-failed",
        detail: "child stdout pipe is unavailable".to_string(),
    })?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let outcome = wait_for_child(&mut child, timeout);
    let output = reader
        .join()
        .map_err(|_| PhaseFailure {
            outcome: "capture-failed",
            detail: "child stdout reader panicked".to_string(),
        })?
        .map_err(|error| PhaseFailure {
            outcome: "capture-failed",
            detail: error.to_string(),
        })?;
    outcome.map(|()| output)
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<(), PhaseFailure> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(PhaseFailure {
                    outcome: "failed",
                    detail: format!("child exited with status {status}"),
                });
            }
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let kill_detail = child
                    .kill()
                    .and_then(|()| child.wait().map(|_| ()))
                    .err()
                    .map_or_else(String::new, |error| format!("; kill failed: {error}"));
                return Err(PhaseFailure {
                    outcome: "timed-out",
                    detail: format!("exceeded {:.0}s{kill_detail}", timeout.as_secs_f64()),
                });
            }
            Err(error) => {
                return Err(PhaseFailure {
                    outcome: "wait-failed",
                    detail: error.to_string(),
                });
            }
        }
    }
}

fn phase_timeout() -> Duration {
    let configured = env::var(PHASE_TIMEOUT_ENV).ok();
    phase_timeout_from(configured.as_deref())
}

fn phase_timeout_from(configured: Option<&str>) -> Duration {
    let seconds = configured
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_PHASE_TIMEOUT_SECONDS);
    Duration::from_secs(seconds)
}

fn report_path() -> PathBuf {
    env::var_os(REPORT_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/rust-test-suite-report.json"))
}

fn write_report(
    path: &Path,
    decision: &str,
    test_threads: usize,
    phase_timeout: Duration,
    wall_time: Duration,
    results: &[PhaseResult],
) -> Result<(), String> {
    let tier_inventory_rows = validate_tier_inventory()?;
    let phases = results
        .iter()
        .map(|result| {
            format!(
                "    {{\"name\":\"{}\",\"tier\":\"{}\",\"executor\":\"{}\",\"outcome\":\"{}\",\"wall_time_ms\":{}}}",
                result.name,
                result.tier.as_str(),
                result.executor,
                result.outcome,
                result.wall_time_ms
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
            "  \"schema\": \"terlan.rust-test-suite.v3\",\n",
            "  \"decision\": \"{}\",\n",
            "  \"closed_stdin\": true,\n",
            "  \"test_threads\": {},\n",
            "  \"phase_timeout_seconds\": {},\n",
            "  \"cargo_invocation_count\": {},\n",
            "  \"cargo_invocation_maximum\": {},\n",
            "  \"wall_time_ms\": {},\n",
            "  \"tier_inventory_path\": \"{}\",\n",
            "  \"tier_inventory_row_count\": {},\n",
            "  \"tier_inventory\": [\n{}\n  ],\n",
            "  \"external_tier_owners\": [\n{}\n  ],\n",
            "  \"phases\": [\n{}\n  ]\n",
            "}}\n"
        ),
        decision,
        test_threads,
        phase_timeout.as_secs(),
        results
            .iter()
            .filter(|result| result.executor.starts_with("cargo"))
            .count(),
        MAX_CARGO_PHASES,
        wall_time.as_millis(),
        TIER_INVENTORY_PATH,
        tier_inventory_rows,
        tiers,
        external_owners,
        phases
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("json.tmp.{}", std::process::id()));
    let write_result = fs::write(&temporary, report).and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot write {}: {error}", path.display()));
    }
    Ok(())
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

fn cargo_program() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn test_path() -> String {
    let root = env::current_dir().unwrap_or_default();
    let current = env::var_os("PATH").unwrap_or_default();
    env::join_paths(std::iter::once(root.join("target/debug")).chain(env::split_paths(&current)))
        .unwrap_or(current)
        .to_string_lossy()
        .into_owned()
}

fn test_phases(coverage_owns_terlc: bool) -> Vec<TestPhase> {
    let mut phases = vec![
        terlan_integration_phase(),
        workspace_support_phase(),
        generated_cpp_package_phase(),
        ignored_std_collection_phase(),
    ];
    if !coverage_owns_terlc {
        phases.insert(0, terlan_library_phase());
    }
    phases
}

fn terlan_library_phase() -> TestPhase {
    let mut args = Vec::new();
    for filter in INTEGRATION_FILTERS {
        args.extend(["--skip", filter]);
    }
    TestPhase {
        name: "Terlan library",
        tier: ValidationTier::FastUnit,
        executor: PhaseExecutor::TerlanHarness,
        args,
    }
}

fn terlan_integration_phase() -> TestPhase {
    let mut args = Vec::new();
    args.extend(INTEGRATION_FILTERS);
    TestPhase {
        name: "Terlan union-feature integration",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::TerlanHarness,
        args,
    }
}

fn workspace_support_phase() -> TestPhase {
    TestPhase {
        name: "workspace support crates",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::Cargo,
        args: vec![
            "test",
            "--locked",
            "--workspace",
            "--exclude",
            "terlan",
            "--",
        ],
    }
}

fn generated_cpp_package_phase() -> TestPhase {
    TestPhase {
        name: "generated C++ package evidence",
        tier: ValidationTier::AotNativeLink,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![
            "commands::bind::cpp_package_consumer_test::generated_cpp_git_package_executes_and_rejects_stale_handles",
            "--ignored",
            "--exact",
        ],
    }
}

fn ignored_std_collection_phase() -> TestPhase {
    TestPhase {
        name: "ignored std collection contract",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![
            "compiler::typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts",
            "--ignored",
            "--exact",
        ],
    }
}
