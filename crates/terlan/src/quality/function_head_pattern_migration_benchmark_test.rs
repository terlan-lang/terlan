use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::*;

fn valid_baseline() -> String {
    let scenarios = SCENARIOS
        .iter()
        .map(|scenario| {
            json!({
                "name": scenario.name,
                "file_count": scenario.file_count,
                "function_declaration_count": scenario.function_declaration_count,
                "candidate_count": scenario.candidate_count,
                "auto_fixed_count": scenario.auto_fixed_count,
                "safe_rejected_count": scenario.safe_rejected_count,
                "elapsed_threshold_ms": scenario.elapsed_threshold_ms,
                "memory_peak_threshold_kb": scenario.memory_peak_threshold_kb
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&json!({
        "schema": REQUIRED_SCHEMA,
        "environment": {
            "allocator": "system",
            "cpu_target": "baseline",
            "rustc": "baseline",
            "terlan_version": "0.0.7"
        },
        "no_regression": {
            "candidate_count_must_match_fixture": true,
            "elapsed_headroom_percent": 25,
            "memory_peak_headroom_percent": 25,
            "sorted_deterministic_artifact": true
        },
        "scenarios": scenarios
    }))
    .expect("serialize valid baseline")
}

fn valid_command() -> String {
    REQUIRED_COMMAND_METRICS.join("\n")
}

fn valid_command_tests() -> String {
    REQUIRED_TEST_EVIDENCE.join("\n")
}

fn valid_makefile() -> String {
    format!(
        "{ASSIST_MAKE_TARGET}:\n\ttrue\n{REQUIRED_MAKE_TARGET}: {ASSIST_MAKE_TARGET}\n\ttrue\n{HARDENING_MAKE_TARGET}:\n\ttrue\n"
    )
}

/// Verifies the benchmark gate writes its deterministic report.
#[test]
fn function_head_pattern_migration_benchmark_writes_report() {
    let repo = TempRepo::new("function_head_pattern_migration_benchmark_writes_report");
    repo.write(BASELINE_PATH, &valid_baseline());
    repo.write(COMMAND_SOURCE, &valid_command());
    repo.write(COMMAND_TEST_SOURCE, &valid_command_tests());
    repo.write(MAKEFILE, &valid_makefile());

    let summary = run_function_head_pattern_migration_benchmark(repo.path())
        .expect("function-head migration benchmark");

    assert_eq!(SCENARIOS.len(), summary.scenario_count);
    assert_eq!(8, summary.metric_count);
    let report = fs::read_to_string(repo.path().join(REPORT_PATH)).expect("read report");
    assert!(report.contains("terlan.function-head-pattern-migration-benchmark-report.v1"));
    assert!(report.contains("candidate counts match checked fixtures"));
}

/// Verifies benchmark fixture count drift fails loudly.
#[test]
fn function_head_pattern_migration_benchmark_rejects_candidate_count_drift() {
    let baseline = valid_baseline().replace("\"candidate_count\": 100", "\"candidate_count\": 99");
    let diagnostics = validate_function_head_pattern_migration_benchmark(
        &baseline,
        &valid_command(),
        &valid_command_tests(),
        &valid_makefile(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("candidate_count")),
        "expected candidate-count diagnostic: {diagnostics:?}"
    );
}

/// Verifies Make ordering remains part of the executable contract.
#[test]
fn function_head_pattern_migration_benchmark_rejects_wrong_make_order() {
    let diagnostics = validate_function_head_pattern_migration_benchmark(
        &valid_baseline(),
        &valid_command(),
        &valid_command_tests(),
        &format!(
            "{REQUIRED_MAKE_TARGET}: {ASSIST_MAKE_TARGET}\n\ttrue\n{ASSIST_MAKE_TARGET}:\n\ttrue\n{HARDENING_MAKE_TARGET}:\n\ttrue\n"
        ),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("after assist and before hardening")),
        "expected Make ordering diagnostic: {diagnostics:?}"
    );
}

/// Verifies the gate rejects missing reproducibility metadata.
#[test]
fn function_head_pattern_migration_benchmark_rejects_missing_environment() {
    let baseline = valid_baseline().replace("\"rustc\": \"baseline\",", "");
    let diagnostics = validate_function_head_pattern_migration_benchmark(
        &baseline,
        &valid_command(),
        &valid_command_tests(),
        &valid_makefile(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("rustc")),
        "expected missing rustc diagnostic: {diagnostics:?}"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, contents: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directory");
        }
        fs::write(path, contents).expect("write fixture");
    }
}
