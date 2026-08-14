#![forbid(unsafe_code)]

use std::env;
use std::process::{Command, ExitCode};
use std::time::Instant;

#[cfg(test)]
#[path = "main_test.rs"]
#[cfg(test)]
mod test_orchestrator_test;

// Library tests exercise process-global compiler paths and environment
// contracts. Parallel libtest threads can make one fixture execute another
// fixture's compiler, so the canonical evidence run is serial by default.
const DEFAULT_TEST_THREADS: usize = 1;
const RELEASE_COVERAGE_OWNS_TERLC_ENV: &str = "TERLAN_RELEASE_COVERAGE_OWNS_TERLC_TESTS";
const VALIDATION_FEATURES: &str = "quality-tools,editor-lsp,benchmark-tools";

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
        println!(
            "[rust-test-suite] release coverage owns the complete normal Terlan library test phase"
        );
    }

    for phase in test_phases(coverage_owns_terlc) {
        let phase_started = Instant::now();
        println!("[rust-test-suite] running {}", phase.name);
        let status = Command::new(cargo_program())
            .args(&phase.args)
            .arg("--test-threads")
            .arg(test_threads.to_string())
            .arg("--quiet")
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
        phase_with_feature_filter("quality", "quality::"),
        phase_with_feature_filter("LSP", "lsp::"),
        phase_with_feature_filter("benchmark harness", "benchmark::"),
        phase_with_feature_filter("cross-feature integration", "comprehension"),
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
    TestPhase {
        name: "Terlan library",
        args: vec!["test", "--locked", "-p", "terlan", "--lib", "--"],
    }
}

fn phase_with_feature_filter(name: &'static str, filter: &'static str) -> TestPhase {
    terlan_feature_phase(name, filter)
}

fn terlan_feature_phase(name: &'static str, filter: &'static str) -> TestPhase {
    TestPhase {
        name,
        args: vec![
            "test",
            "--locked",
            "-p",
            "terlan",
            "--lib",
            "--features",
            VALIDATION_FEATURES,
            filter,
            "--",
        ],
    }
}

fn workspace_support_phase() -> TestPhase {
    TestPhase {
        name: "workspace support crates",
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
        args: vec![
            "test",
            "--locked",
            "-p",
            "terlan",
            "--lib",
            "commands::bind::cpp_package_consumer_test::generated_cpp_git_package_executes_and_rejects_stale_handles",
            "--",
            "--ignored",
            "--exact",
        ],
    }
}

fn ignored_std_collection_phase() -> TestPhase {
    TestPhase {
        name: "ignored std collection contract",
        args: vec![
            "test",
            "--locked",
            "-p",
            "terlan",
            "--lib",
            "compiler::typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts",
            "--",
            "--ignored",
            "--exact",
        ],
    }
}
