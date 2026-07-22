use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const COMMAND_SOURCE: &str = "crates/terlan/src/commands/migrate/mod.rs";
const COMMAND_TEST_SOURCE: &str = "crates/terlan/src/commands/migrate/migrate_test.rs";
const MAIN_SOURCE: &str = "crates/terlan/src/main.rs";
const MAKEFILE: &str = "Makefile";
const MANIFEST_PATH: &str = "target/quality/function-head-pattern-migration-manifest.json";
const REPORT_PATH: &str = "target/quality/function-head-pattern-migration-assist-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "terlc migrate pattern-head [--write] [--json]",
    "run_pattern_head_migration",
    "migration.function_head_pattern.invalid_alias_style",
    "terlan.function-head-pattern-migration-assist-report.v1",
    "SafeRejected",
    "planned_count",
    "applied_count",
];

const REQUIRED_TESTS: &[&str] = &[
    "pattern_head_migration_dry_run_reports_plan_without_writing",
    "pattern_head_migration_write_rewrites_safe_reverse_alias",
    "pattern_head_migration_safe_rejects_ambiguous_alias_shape",
    "pattern_head_migration_is_idempotent_for_pattern_first_heads",
    "pattern_head_migration_json_report_uses_stable_schema_and_ids",
];

const REQUIRED_MAKE_TARGETS: &[&str] = &[
    "function-head-pattern-migration-assist-check",
    "function-head-migration-lint-check",
    "function-head-pattern-parameters-hardening-check",
];

/// Summary produced by the function-head migration assist gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadPatternMigrationAssistSummary {
    pub required_term_count: usize,
    pub required_test_count: usize,
    pub make_target_count: usize,
    pub report_path: String,
}

/// Runs the function-head pattern migration assist quality gate.
pub fn run_function_head_pattern_migration_assist(
    root: &Path,
) -> QualityResult<FunctionHeadPatternMigrationAssistSummary> {
    let command = read_required_file(root, COMMAND_SOURCE)?;
    let command_tests = read_required_file(root, COMMAND_TEST_SOURCE)?;
    let main = read_required_file(root, MAIN_SOURCE)?;
    let makefile = read_required_file(root, MAKEFILE)?;
    let manifest = read_required_file(root, MANIFEST_PATH)?;

    let diagnostics = validate_function_head_pattern_migration_assist(
        &command,
        &command_tests,
        &main,
        &makefile,
        &manifest,
    );
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;

    Ok(FunctionHeadPatternMigrationAssistSummary {
        required_term_count: REQUIRED_TERMS.len(),
        required_test_count: REQUIRED_TESTS.len(),
        make_target_count: REQUIRED_MAKE_TARGETS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn read_required_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read migration assist input: {err}",
            path.display()
        )
    })
}

fn validate_function_head_pattern_migration_assist(
    command: &str,
    command_tests: &str,
    main: &str,
    makefile: &str,
    manifest: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();

    let combined_command = format!("{command}\n{main}");
    for term in REQUIRED_TERMS {
        if !combined_command.contains(term) {
            diagnostics.push(format!("migration assist command is missing `{term}`"));
        }
    }

    for test in REQUIRED_TESTS {
        if !command_tests.contains(test) {
            diagnostics.push(format!("migration assist tests are missing `{test}`"));
        }
    }

    for target in REQUIRED_MAKE_TARGETS {
        if !makefile.contains(target) {
            diagnostics.push(format!(
                "Makefile is missing migration assist target `{target}`"
            ));
        }
    }

    if !manifest.contains("migration.function_head_pattern.invalid_alias_style") {
        diagnostics.push("migration manifest is missing invalid-alias migration ID".to_string());
    }
    if !main.contains("\"migrate\" => commands::migrate::run(cmd)") {
        diagnostics.push("public CLI dispatcher does not route `terlc migrate`".to_string());
    }

    diagnostics
}

fn write_report(path: &Path) -> QualityResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create report dir: {err}", parent.display()))?;
    }
    let report = json!({
        "schema": "terlan.function-head-pattern-migration-assist-gate.v1",
        "command": "terlc migrate pattern-head",
        "migration_ids": ["migration.function_head_pattern.invalid_alias_style"],
        "contracts": [
            "dry-run is default",
            "write mode is explicit",
            "ambiguous candidates are safe-rejected",
            "already-migrated input is idempotent",
            "json output reuses manifest migration IDs"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize migration assist report: {err}"))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write report: {err}", path.display()))
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-pattern-migration-assist] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_pattern_migration_assist_test.rs"]
mod function_head_pattern_migration_assist_test;
