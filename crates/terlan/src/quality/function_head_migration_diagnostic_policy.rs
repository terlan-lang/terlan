use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const FUNCTION_HEAD_MIGRATION_POLICY_DOC: &str =
    "docs/language/FUNCTION_HEAD_MIGRATION_DIAGNOSTIC_POLICY.md";
const REPORT_PATH: &str = "target/quality/function-head-migration-diagnostic-policy-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "function-head pattern migration",
    "all targets",
    "migration diagnostic namespace",
    "migration.function_head_pattern.invalid_alias_style",
    "migration.function_head_pattern.safe_reject",
    "migration.function_head_pattern.unsupported_backend",
    "stable code",
    "stable family",
    "CLI formats",
    "text format",
    "JSON format",
    "CI and tools",
    "exact migration outcomes",
    "VM allows all accepted rewrite-safe patterns",
    "JS target emits explicit unsupported-migration diagnostics",
    "alter backend behavior",
    "Editor",
    "lsp",
    "formatter",
    "tree-sitter smoke outputs",
    "same migration IDs",
    "same source shape",
    "compatibility matrix",
    "parser_accept",
    "typecheck_diagnose",
    "formatter_stable",
    "vm_lower",
    "js_reject",
    "parser acceptance",
    "typecheck warning migration row",
    "VM runtime parity",
    "strict profile escalation",
    "warning to error",
    "same migration ID",
    "JS profile-specific rejection",
    "explicit policy-family diagnostic",
    "reserved namespace",
    "No implicit numeric fallback codes",
    "roadmap update",
    "executable snapshot update",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "implicit numeric fallback codes are allowed",
    "migration diagnostics may omit the reserved namespace",
    "text format may use different migration ids",
    "json format may use different migration ids",
    "js target may silently accept unsupported migration",
    "policy matrix columns can drift without roadmap update",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the function-head migration diagnostic policy gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadMigrationDiagnosticPolicySummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the function-head migration diagnostic policy gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/language/`.
///
/// Output:
/// - Success summary and report when migration diagnostic IDs, CLI formats,
///   target-profile behavior, tooling surfaces, and the compatibility matrix
///   are documented.
/// - Stable diagnostics when policy evidence is missing.
///
/// Transformation:
/// - Converts the function-head migration diagnostic policy into executable
///   release evidence for the 0.0.7 roadmap.
pub fn run_function_head_migration_diagnostic_policy(
    root: &Path,
) -> QualityResult<FunctionHeadMigrationDiagnosticPolicySummary> {
    let path = root.join(FUNCTION_HEAD_MIGRATION_POLICY_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read function-head migration diagnostic policy: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_function_head_migration_diagnostic_policy_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(FunctionHeadMigrationDiagnosticPolicySummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_function_head_migration_diagnostic_policy_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!(
                "missing function-head migration diagnostic policy term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden function-head migration diagnostic policy claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder function-head migration diagnostic policy text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create function-head migration policy report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.function-head-migration-diagnostic-policy.v1",
        "artifact_evidence": "function-head migration diagnostic policy",
        "reserved_namespace": [
            "migration.function_head_pattern.invalid_alias_style",
            "migration.function_head_pattern.safe_reject",
            "migration.function_head_pattern.unsupported_backend"
        ],
        "cli_formats": [
            "text format",
            "JSON format",
            "stable code",
            "stable family"
        ],
        "target_profiles": [
            "VM allows all accepted rewrite-safe patterns",
            "JS target emits explicit unsupported-migration diagnostics"
        ],
        "tooling_surfaces": [
            "Editor",
            "lsp",
            "formatter",
            "tree-sitter smoke outputs"
        ],
        "compatibility_matrix": [
            "parser_accept",
            "typecheck_diagnose",
            "formatter_stable",
            "vm_lower",
            "js_reject"
        ]
    });
    let text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize function-head migration policy report: {err}")
    })?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write function-head migration policy report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-migration-diagnostic-policy] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_migration_diagnostic_policy_test.rs"]
#[cfg(test)]
mod function_head_migration_diagnostic_policy_test;
