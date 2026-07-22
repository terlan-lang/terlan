use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::terlan_quality::QualityResult;

const BASELINE_PATH: &str = "benchmarks/roadmap/function_head_pattern_migration_bench.latest.json";
const COMMAND_SOURCE: &str = "crates/terlan/src/commands/migrate/mod.rs";
const COMMAND_TEST_SOURCE: &str = "crates/terlan/src/commands/migrate/migrate_test.rs";
const MAKEFILE: &str = "Makefile";
const REPORT_PATH: &str = "target/quality/function-head-pattern-migration-benchmark-report.json";

const REQUIRED_SCHEMA: &str = "terlan.function-head-pattern-migration-benchmark.v1";
const REQUIRED_MAKE_TARGET: &str = "function-head-pattern-migration-benchmark-check";
const ASSIST_MAKE_TARGET: &str = "function-head-pattern-migration-assist-check";
const HARDENING_MAKE_TARGET: &str = "function-head-pattern-parameters-hardening-check";

const REQUIRED_COMMAND_METRICS: &[&str] = &[
    "planned_count",
    "applied_count",
    "safe_rejected_count",
    "changed_file_count",
];

const REQUIRED_TEST_EVIDENCE: &[&str] = &[
    "pattern_head_migration_dry_run_reports_plan_without_writing",
    "pattern_head_migration_write_rewrites_safe_reverse_alias",
    "pattern_head_migration_safe_rejects_ambiguous_alias_shape",
    "pattern_head_migration_is_idempotent_for_pattern_first_heads",
];

const REQUIRED_ENVIRONMENT_KEYS: &[&str] = &["rustc", "cpu_target", "allocator", "terlan_version"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BenchmarkScenario {
    name: &'static str,
    file_count: u64,
    function_declaration_count: u64,
    candidate_count: u64,
    auto_fixed_count: u64,
    safe_rejected_count: u64,
    elapsed_threshold_ms: u64,
    memory_peak_threshold_kb: u64,
}

const SCENARIOS: &[BenchmarkScenario] = &[
    BenchmarkScenario {
        name: "large-module-0100",
        file_count: 1,
        function_declaration_count: 100,
        candidate_count: 100,
        auto_fixed_count: 80,
        safe_rejected_count: 20,
        elapsed_threshold_ms: 250,
        memory_peak_threshold_kb: 4096,
    },
    BenchmarkScenario {
        name: "large-module-0500",
        file_count: 5,
        function_declaration_count: 500,
        candidate_count: 500,
        auto_fixed_count: 400,
        safe_rejected_count: 100,
        elapsed_threshold_ms: 900,
        memory_peak_threshold_kb: 8192,
    },
    BenchmarkScenario {
        name: "large-module-1000",
        file_count: 10,
        function_declaration_count: 1000,
        candidate_count: 1000,
        auto_fixed_count: 800,
        safe_rejected_count: 200,
        elapsed_threshold_ms: 1800,
        memory_peak_threshold_kb: 16384,
    },
    BenchmarkScenario {
        name: "mixed-validity-recovery",
        file_count: 4,
        function_declaration_count: 80,
        candidate_count: 64,
        auto_fixed_count: 48,
        safe_rejected_count: 16,
        elapsed_threshold_ms: 400,
        memory_peak_threshold_kb: 4096,
    },
    BenchmarkScenario {
        name: "repeated-memory-pressure",
        file_count: 20,
        function_declaration_count: 2000,
        candidate_count: 2000,
        auto_fixed_count: 1600,
        safe_rejected_count: 400,
        elapsed_threshold_ms: 3200,
        memory_peak_threshold_kb: 24576,
    },
];

/// Summary produced by the function-head migration benchmark gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadPatternMigrationBenchmarkSummary {
    pub scenario_count: usize,
    pub metric_count: usize,
    pub report_path: String,
}

/// Runs the deterministic migration benchmark contract gate.
///
/// Inputs:
/// - `root`: repository root containing `benchmarks/roadmap/` and `Makefile`.
///
/// Output:
/// - Success summary and generated report when checked baselines, command
///   counters, and Make ordering agree.
/// - Stable diagnostics when benchmark fixture counts or thresholds drift.
///
/// Transformation:
/// - Converts the Slice 10 benchmark requirements into a reproducible baseline
///   contract that CI can enforce without wall-clock flake.
pub fn run_function_head_pattern_migration_benchmark(
    root: &Path,
) -> QualityResult<FunctionHeadPatternMigrationBenchmarkSummary> {
    let baseline = read_required_file(root, BASELINE_PATH)?;
    let command = read_required_file(root, COMMAND_SOURCE)?;
    let command_tests = read_required_file(root, COMMAND_TEST_SOURCE)?;
    let makefile = read_required_file(root, MAKEFILE)?;

    let diagnostics = validate_function_head_pattern_migration_benchmark(
        &baseline,
        &command,
        &command_tests,
        &makefile,
    );
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;

    Ok(FunctionHeadPatternMigrationBenchmarkSummary {
        scenario_count: SCENARIOS.len(),
        metric_count: 8,
        report_path: REPORT_PATH.to_string(),
    })
}

fn read_required_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read migration benchmark input: {err}",
            path.display()
        )
    })
}

fn validate_function_head_pattern_migration_benchmark(
    baseline: &str,
    command: &str,
    command_tests: &str,
    makefile: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();

    for metric in REQUIRED_COMMAND_METRICS {
        if !command.contains(metric) {
            diagnostics.push(format!("migration command no longer records `{metric}`"));
        }
    }
    for test in REQUIRED_TEST_EVIDENCE {
        if !command_tests.contains(test) {
            diagnostics.push(format!(
                "migration benchmark lost command evidence `{test}`"
            ));
        }
    }

    validate_makefile(makefile, &mut diagnostics);
    validate_baseline(baseline, &mut diagnostics);

    diagnostics
}

fn validate_makefile(makefile: &str, diagnostics: &mut Vec<String>) {
    for target in [
        REQUIRED_MAKE_TARGET,
        ASSIST_MAKE_TARGET,
        HARDENING_MAKE_TARGET,
    ] {
        if !makefile.contains(target) {
            diagnostics.push(format!(
                "missing migration benchmark Make target `{target}`"
            ));
        }
    }

    let dependency_line = format!("{REQUIRED_MAKE_TARGET}: {ASSIST_MAKE_TARGET}");
    if !makefile.contains(&dependency_line) {
        diagnostics.push(format!(
            "migration benchmark target must depend directly on `{ASSIST_MAKE_TARGET}`"
        ));
    }

    let assist_index = find_make_target(makefile, ASSIST_MAKE_TARGET);
    let benchmark_index = find_make_target(makefile, REQUIRED_MAKE_TARGET);
    let hardening_index = find_make_target(makefile, HARDENING_MAKE_TARGET);
    if let (Some(assist), Some(benchmark), Some(hardening)) =
        (assist_index, benchmark_index, hardening_index)
    {
        if !(assist < benchmark && benchmark < hardening) {
            diagnostics.push(format!(
                "`{REQUIRED_MAKE_TARGET}` must run after assist and before hardening"
            ));
        }
    }
}

fn find_make_target(makefile: &str, target: &str) -> Option<usize> {
    let needle = format!("\n{target}:");
    makefile
        .find(&needle)
        .map(|index| index + 1)
        .or_else(|| makefile.strip_prefix(&format!("{target}:")).map(|_| 0))
}

fn validate_baseline(baseline: &str, diagnostics: &mut Vec<String>) {
    let parsed = match serde_json::from_str::<Value>(baseline) {
        Ok(parsed) => parsed,
        Err(err) => {
            diagnostics.push(format!("benchmark baseline is not valid JSON: {err}"));
            return;
        }
    };

    if parsed.get("schema").and_then(Value::as_str) != Some(REQUIRED_SCHEMA) {
        diagnostics.push(format!(
            "benchmark baseline must use schema `{REQUIRED_SCHEMA}`"
        ));
    }

    let environment = parsed.get("environment").and_then(Value::as_object);
    match environment {
        Some(environment) => {
            for key in REQUIRED_ENVIRONMENT_KEYS {
                if !environment.contains_key(*key) {
                    diagnostics.push(format!("benchmark baseline missing environment `{key}`"));
                }
            }
        }
        None => diagnostics.push("benchmark baseline missing environment metadata".to_string()),
    }

    let no_regression = parsed.get("no_regression").and_then(Value::as_object);
    if no_regression
        .and_then(|contract| contract.get("candidate_count_must_match_fixture"))
        .and_then(Value::as_bool)
        != Some(true)
    {
        diagnostics
            .push("benchmark baseline must assert candidate counts match fixtures".to_string());
    }

    let Some(scenarios) = parsed.get("scenarios").and_then(Value::as_array) else {
        diagnostics.push("benchmark baseline missing scenarios array".to_string());
        return;
    };
    if scenarios.len() != SCENARIOS.len() {
        diagnostics.push(format!(
            "benchmark baseline scenario count drifted: expected {}, found {}",
            SCENARIOS.len(),
            scenarios.len()
        ));
        return;
    }

    let mut previous_name = "";
    for (index, expected) in SCENARIOS.iter().enumerate() {
        let scenario = &scenarios[index];
        let name = scenario.get("name").and_then(Value::as_str).unwrap_or("");
        if name < previous_name {
            diagnostics.push("benchmark scenarios must be sorted by name".to_string());
        }
        previous_name = name;
        validate_scenario(scenario, expected, diagnostics);
    }
}

fn validate_scenario(
    scenario: &Value,
    expected: &BenchmarkScenario,
    diagnostics: &mut Vec<String>,
) {
    let actual_name = scenario.get("name").and_then(Value::as_str).unwrap_or("");
    if actual_name != expected.name {
        diagnostics.push(format!(
            "benchmark scenario name drifted: expected `{}`, found `{actual_name}`",
            expected.name
        ));
        return;
    }

    for (field, expected_value) in [
        ("file_count", expected.file_count),
        (
            "function_declaration_count",
            expected.function_declaration_count,
        ),
        ("candidate_count", expected.candidate_count),
        ("auto_fixed_count", expected.auto_fixed_count),
        ("safe_rejected_count", expected.safe_rejected_count),
        ("elapsed_threshold_ms", expected.elapsed_threshold_ms),
        (
            "memory_peak_threshold_kb",
            expected.memory_peak_threshold_kb,
        ),
    ] {
        if scenario.get(field).and_then(Value::as_u64) != Some(expected_value) {
            diagnostics.push(format!(
                "benchmark scenario `{}` field `{field}` drifted from generated fixture",
                expected.name
            ));
        }
    }

    if expected.auto_fixed_count + expected.safe_rejected_count != expected.candidate_count {
        diagnostics.push(format!(
            "benchmark scenario `{}` fixture counts do not add up",
            expected.name
        ));
    }
    if expected.elapsed_threshold_ms == 0 || expected.memory_peak_threshold_kb == 0 {
        diagnostics.push(format!(
            "benchmark scenario `{}` must keep positive no-regression thresholds",
            expected.name
        ));
    }
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create migration benchmark report directory: {err}",
                parent.display()
            )
        })?;
    }

    let report = json!({
        "schema": "terlan.function-head-pattern-migration-benchmark-report.v1",
        "baseline": BASELINE_PATH,
        "scenarios": SCENARIOS.iter().map(|scenario| {
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
        }).collect::<Vec<_>>(),
        "stability_contracts": [
            "candidate counts match checked fixtures",
            "dry-run/write/idempotent command tests remain wired",
            "Make target runs after assist and before hardening",
            "artifact is sorted and deterministic"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize migration benchmark report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write migration benchmark report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-pattern-migration-benchmark] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_pattern_migration_benchmark_test.rs"]
mod function_head_pattern_migration_benchmark_test;
