#![forbid(unsafe_code)]

use std::env;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[cfg(test)]
#[path = "test_orchestrator_test.rs"]
mod test_orchestrator_test;

const DEFAULT_TEST_THREADS: usize = 8;
const RELEASE_COVERAGE_OWNS_TERLC_ENV: &str = "TERLAN_RELEASE_COVERAGE_OWNS_TERLC_TESTS";

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestPhase {
    name: &'static str,
    args: Vec<&'static str>,
}

fn main() -> ExitCode {
    let test_threads = env::var("TERLAN_TEST_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or(DEFAULT_TEST_THREADS);
    let started = Instant::now();
    let coverage_owns_terlc = env::var(RELEASE_COVERAGE_OWNS_TERLC_ENV).as_deref() == Ok("1");

    if coverage_owns_terlc {
        println!("[rust-test-suite] release coverage owns the complete normal terlc test phase");
    }

    for phase in test_phases(coverage_owns_terlc) {
        let phase_started = Instant::now();
        println!("[rust-test-suite] running {}", phase.name);
        let status = Command::new(cargo_program())
            .args(&phase.args)
            .arg("--test-threads")
            .arg(test_threads.to_string())
            .env("PATH", test_path())
            .status();
        match status {
            Ok(status) if status.success() => println!(
                "[rust-test-suite] {} passed in {:.2}s",
                phase.name,
                phase_started.elapsed().as_secs_f64()
            ),
            Ok(status) => {
                eprintln!(
                    "[rust-test-suite] {} failed with status {status}",
                    phase.name
                );
                return ExitCode::from(1);
            }
            Err(error) => {
                eprintln!("[rust-test-suite] failed to launch {}: {error}", phase.name);
                return ExitCode::from(1);
            }
        }
    }

    println!(
        "[rust-test-suite] all owned harnesses passed in {:.2}s",
        started.elapsed().as_secs_f64()
    );
    ExitCode::SUCCESS
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
        phase(
            "terlan-vm owned tests",
            "terlan-vm",
            &[
                "--skip",
                "compiler::",
                "--skip",
                "commands::",
                "--skip",
                "formal_pipeline::",
                "--skip",
                "html::",
                "--skip",
                "mobile::",
                "--skip",
                "runtime::",
                "--skip",
                "validation::",
            ],
        ),
        phase_with_filter("quality", "terlan-quality", "terlan_quality::"),
        phase_with_feature_filter("LSP", "terlan-lsp", "editor-lsp", "terlan_lsp::"),
        phase_with_filter("benchmark harness", "terlan-benchmark", "tests::"),
        phase(
            "native target feasibility",
            "terlan-native-target-feasibility",
            &[],
        ),
        phase("Lean proof closeout", "terlan-lean-proof-closeout", &[]),
        TestPhase {
            name: "ignored std collection contract",
            args: vec![
                "test",
                "--locked",
                "-p",
                "terlan",
                "--bin",
                "terlc",
                "compiler::typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts",
                "--",
                "--ignored",
                "--exact",
            ],
        },
    ];
    if !coverage_owns_terlc {
        phases.insert(0, phase("terlc", "terlc", &[]));
    }
    phases
}

fn phase(name: &'static str, binary: &'static str, harness_args: &[&'static str]) -> TestPhase {
    let mut args = vec!["test", "--locked", "-p", "terlan", "--bin", binary, "--"];
    args.extend_from_slice(harness_args);
    TestPhase { name, args }
}

fn phase_with_filter(name: &'static str, binary: &'static str, filter: &'static str) -> TestPhase {
    let mut phase = phase(name, binary, &[]);
    phase.args.insert(6, filter);
    phase
}

fn phase_with_feature_filter(
    name: &'static str,
    binary: &'static str,
    feature: &'static str,
    filter: &'static str,
) -> TestPhase {
    let mut phase = phase_with_filter(name, binary, filter);
    phase.args.splice(4..4, ["--features", feature]);
    phase
}
