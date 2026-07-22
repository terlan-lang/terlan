use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const MIGRATION_POLICY_DOC: &str = "docs/language/FUNCTION_HEAD_MIGRATION_DIAGNOSTIC_POLICY.md";
const MIGRATION_GUIDE_DOC: &str = "docs/language/function_heads.md";
const ROADMAP_DOC: &str = "../docs/roadmap/ROADMAP_0_0_7.md";
const RELEASE_NOTES_DOC: &str = "../docs/roadmap/RELEASE_NOTES_0_0_7.md";
const HANDOFF_REPORT_PATH: &str = "target/quality/function-head-pattern-handoff-report.json";

const REQUIRED_GATES: &[&str] = &[
    "function-head-migration-diagnostic-policy-check",
    "function-head-migration-lint-check",
    "function-head-pattern-migration-benchmark-check",
    "function-head-pattern-parameters-check",
    "function-head-pattern-migration-docs-check",
    "function-head-pattern-0-0-7-handoff-check",
];

const CLOSURE_MATRIX_ROWS: &[&str] = &[
    "parser/parser-rewrite",
    "migration lint",
    "migration assist",
    "migration benchmark",
    "diagnostics policy",
    "docs/deprecation closeout",
    "parser",
    "typecheck",
    "formatter",
    "VM/runtime",
    "Javascript-profile",
];

const PERMANENT_BEHAVIOR_TERMS: &[&str] = &[
    "accepted with warning",
    "strict mode",
    "migration.function_head_pattern.invalid_alias_style",
    "migration.function_head_pattern.safe_reject",
    "migration.function_head_pattern.unsupported_backend",
    "migration.function_head_pattern.remains",
];

const FORBIDDEN_HANDOFF_CLAIMS: &[&str] = &[
    "temporary migration-only docs/command flags remain",
    "handoff can pass with stale artifacts",
    "normal default-path codepaths use compatibility shims",
    "closure metrics are optional",
];

/// Summary produced by the final function-head pattern handoff gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadPatternHandoffSummary {
    pub required_gate_count: usize,
    pub closure_matrix_row_count: usize,
    pub report_path: String,
}

/// Runs the final function-head pattern 0.0.7 handoff gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/language/`.
///
/// Output:
/// - Success summary and handoff manifest after the Make target has run the
///   prerequisite function-head gates.
/// - Stable diagnostics when docs, release notes, or closure matrix evidence
///   drift from the 0.0.7 handoff contract.
///
/// Transformation:
/// - Converts the feature handoff from scattered gate evidence into one
///   machine-readable release manifest.
pub fn run_function_head_pattern_handoff(
    root: &Path,
) -> QualityResult<FunctionHeadPatternHandoffSummary> {
    let migration_policy = read_required_file(root, MIGRATION_POLICY_DOC)?;
    let migration_guide = read_required_file(root, MIGRATION_GUIDE_DOC)?;
    let roadmap = read_required_file(root, ROADMAP_DOC)?;
    let release_notes = read_required_file(root, RELEASE_NOTES_DOC)?;

    let diagnostics = validate_function_head_pattern_handoff_texts(
        &migration_policy,
        &migration_guide,
        &roadmap,
        &release_notes,
    );
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let report_path = root.join(HANDOFF_REPORT_PATH);
    write_report(&report_path)?;
    Ok(FunctionHeadPatternHandoffSummary {
        required_gate_count: REQUIRED_GATES.len(),
        closure_matrix_row_count: CLOSURE_MATRIX_ROWS.len(),
        report_path: HANDOFF_REPORT_PATH.to_string(),
    })
}

fn read_required_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read function-head pattern handoff input: {err}",
            path.display()
        )
    })
}

fn validate_function_head_pattern_handoff_texts(
    migration_policy: &str,
    migration_guide: &str,
    roadmap: &str,
    release_notes: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let combined = [migration_policy, migration_guide, roadmap, release_notes].join("\n");
    let normalized_combined = combined.to_lowercase();

    for gate in REQUIRED_GATES {
        if !combined.contains(gate) {
            diagnostics.push(format!("missing function-head handoff gate `{gate}`"));
        }
    }
    for row in CLOSURE_MATRIX_ROWS {
        if !combined.contains(row) {
            diagnostics.push(format!("missing function-head closure matrix row `{row}`"));
        }
    }
    for term in PERMANENT_BEHAVIOR_TERMS {
        if !combined.contains(term) {
            diagnostics.push(format!("missing retained permanent behavior term `{term}`"));
        }
    }
    for claim in FORBIDDEN_HANDOFF_CLAIMS {
        if normalized_combined.contains(&claim.to_lowercase()) {
            diagnostics.push(format!("forbidden function-head handoff claim `{claim}`"));
        }
    }
    if !roadmap.contains("feature from") || !roadmap.contains("in progress") {
        diagnostics
            .push("roadmap handoff slice must retain release checklist status wording".to_string());
    }
    if !release_notes.contains("function-head pattern migration closeout docs") {
        diagnostics.push("release notes must anchor function-head closeout evidence".to_string());
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create function-head handoff report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.function-head-pattern-handoff.v1",
        "feature": "function-head pattern parameters",
        "release": "0.0.7",
        "status": "implemented_when_handoff_gate_is_green",
        "required_gates": REQUIRED_GATES.iter().map(|gate| {
            json!({
                "name": gate,
                "status": "passed_before_manifest_write",
                "timing_snapshot": "captured by make prerequisite execution"
            })
        }).collect::<Vec<_>>(),
        "closure_matrix": CLOSURE_MATRIX_ROWS.iter().map(|row| {
            json!({
                "row": row,
                "status": "recorded"
            })
        }).collect::<Vec<_>>(),
        "compatibility_shims": {
            "normal_default_path_usage": "rejected",
            "retention_policy": "explicit version gate only"
        },
        "manifest": HANDOFF_REPORT_PATH
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize function-head handoff report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write function-head handoff report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-pattern-handoff] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_pattern_handoff_test.rs"]
mod function_head_pattern_handoff_test;
