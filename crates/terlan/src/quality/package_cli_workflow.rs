use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_CLI_WORKFLOW_DOC: &str = "docs/package/TERLAN_PACKAGE_CLI_WORKFLOW.md";
const REPORT_PATH: &str = "target/quality/package-cli-workflow-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "installed release artifacts",
    "clean temporary workspaces",
    "preserve user files",
    "explicit write action",
    "terlc package add",
    "terlc package remove",
    "terlc package update",
    "terlc package tree",
    "terlc package audit",
    "terlc package publish --dry-run",
    "terlc package cache clean --check",
    "manifests",
    "lockfiles",
    "deterministically",
    "Text output",
    "JSON output",
    "Network access is disabled",
    "live registry",
    "target constraints",
    "capabilities",
    "native artifacts",
    "generated bindings",
    "yanked packages",
    "duplicate versions",
    "security warnings",
    "provenance warnings",
    "adding incompatible packages",
    "removing transitive dependencies",
    "update conflicts",
    "stale lockfiles",
    "malformed package specs",
    "cache poisoning",
    "source-path leakage",
    "JSON/text output drift",
    "write operations without explicit consent",
    "package-cli-workflow-report.json",
    "command matrix",
    "before/after manifest hashes",
    "lockfile hashes",
    "output snapshots",
    "diagnostics",
    "cache behavior",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "network access is implicit",
    "write operations are implicit",
    "json output may drift from text output",
    "package commands require workspace paths",
    "cache poisoning is ignored",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package CLI workflow gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCliWorkflowSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package CLI workflow gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when deterministic package CLI workflow
///   semantics are documented.
/// - Stable diagnostics when command surfaces, output guarantees, cache
///   behavior, or adversarial workflow cases are missing.
///
/// Transformation:
/// - Converts the package CLI workflow contract into executable release evidence
///   for the 0.0.7 package-system roadmap.
pub fn run_package_cli_workflow(root: &Path) -> QualityResult<PackageCliWorkflowSummary> {
    let path = root.join(PACKAGE_CLI_WORKFLOW_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package CLI workflow contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_cli_workflow_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageCliWorkflowSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_cli_workflow_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package CLI workflow term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package CLI workflow claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package CLI workflow text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package CLI workflow report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-cli-workflow.v1",
        "cli_evidence": "deterministic package CLI workflow contract",
        "commands": [
            "terlc package add",
            "terlc package remove",
            "terlc package update",
            "terlc package tree",
            "terlc package audit",
            "terlc package publish --dry-run",
            "terlc package cache clean --check"
        ],
        "output_snapshots": [
            "text output",
            "JSON output",
            "diagnostics"
        ],
        "adversarial_workflows": [
            "adding incompatible packages",
            "removing transitive dependencies",
            "update conflicts",
            "stale lockfiles",
            "yanked packages",
            "malformed package specs",
            "cache poisoning",
            "source-path leakage",
            "JSON/text output drift",
            "write operations without explicit consent"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package CLI workflow report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package CLI workflow report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-cli-workflow] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_cli_workflow_test.rs"]
mod package_cli_workflow_test;
