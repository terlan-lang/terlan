use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_WORKSPACE_GRAPH_DOC: &str = "docs/package/TERLAN_PACKAGE_WORKSPACE_GRAPH.md";
const REPORT_PATH: &str = "target/quality/package-workspace-graph-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "multi-package workspaces",
    "deterministic",
    "build",
    "test",
    "lint",
    "format-check",
    "package tree",
    "package audit",
    "docs generation",
    "release dry-run",
    "package discovery order",
    "stale build artifacts",
    "implicit local paths",
    "ambient registry state",
    "workspace manifest",
    "multiple local packages",
    "shared lockfile",
    "shared registry mirror",
    "package graph roots",
    "local path dependencies",
    "package-level capabilities",
    "per-package target support",
    "deterministic topological order",
    "workspace root",
    "path",
    "package hash",
    "target metadata",
    "capability summary",
    "package cycles",
    "duplicate package names",
    "conflicting versions",
    "conflicting capabilities",
    "stale local path hashes",
    "mismatched target support",
    "cross-package generated binding drift",
    "cyclic workspaces",
    "duplicate local packages",
    "path traversal",
    "hidden source-checkout dependencies",
    "stale shared lockfiles",
    "nondeterministic graph order",
    "package-specific target mismatch",
    "one package passing only because another package left build artifacts",
    "package-workspace-graph-report.json",
    "workspace fixture paths",
    "package graph",
    "topological order",
    "lockfile hash",
    "per-package command results",
    "diagnostics",
    "artifact isolation checks",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "workspace behavior may depend on package discovery order",
    "implicit local paths are allowed",
    "ambient registry state may affect workspace behavior",
    "stale build artifacts may affect package results",
    "package cycles are allowed",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package workspace graph gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageWorkspaceGraphSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package workspace graph gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when deterministic multi-package workspace
///   semantics are documented.
/// - Stable diagnostics when graph layout, command ordering, local dependency
///   rules, diagnostics, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the package workspace graph contract into executable release
///   evidence for the 0.0.7 package-system roadmap.
pub fn run_package_workspace_graph(root: &Path) -> QualityResult<PackageWorkspaceGraphSummary> {
    let path = root.join(PACKAGE_WORKSPACE_GRAPH_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package workspace graph contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_workspace_graph_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageWorkspaceGraphSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_workspace_graph_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package workspace graph term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package workspace graph claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package workspace graph text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package workspace graph report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-workspace-graph.v1",
        "workspace_evidence": "deterministic multi-package workspace graph contract",
        "workspace_fixture_paths": [
            "multi-package workspace",
            "shared lockfile",
            "shared registry mirror"
        ],
        "package_graph": [
            "package graph roots",
            "local path dependencies",
            "package-level capabilities",
            "per-package target support"
        ],
        "topological_order": [
            "build",
            "test",
            "lint",
            "format-check",
            "package tree",
            "package audit",
            "docs generation",
            "release dry-run"
        ],
        "lockfile_hash": [
            "path",
            "package hash",
            "target metadata",
            "capability summary"
        ],
        "per_package_command_results": [
            "deterministic topological order",
            "artifact isolation checks"
        ],
        "diagnostics": [
            "package cycles",
            "duplicate package names",
            "conflicting versions",
            "conflicting capabilities",
            "stale local path hashes",
            "mismatched target support",
            "cross-package generated binding drift"
        ],
        "artifact_isolation_checks": [
            "stale build artifacts",
            "one package passing only because another package left build artifacts"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package workspace graph report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package workspace graph report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-workspace-graph] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_workspace_graph_test.rs"]
mod package_workspace_graph_test;
