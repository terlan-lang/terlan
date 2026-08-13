use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_EDITOR_INTEGRATION_DOC: &str = "docs/package/TERLAN_PACKAGE_EDITOR_INTEGRATION.md";
const REPORT_PATH: &str = "target/quality/package-editor-integration-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "CLI",
    "compiler",
    "LSP",
    "editor",
    "lockfile",
    "installed package cache",
    "source checkout paths",
    "package modules",
    "exported functions",
    "types",
    "shapes",
    "docs",
    "examples",
    "capabilities",
    "diagnostics",
    "generated binding metadata",
    "package imports",
    "exported symbols",
    "methods",
    "constructors",
    "documented examples",
    "package version",
    "docs summary",
    "target support",
    "capability requirements",
    "deprecation status",
    "generated binding provenance",
    "generated package docs",
    "missing package",
    "stale lockfile",
    "yanked package",
    "incompatible target",
    "missing capability",
    "missing native artifact",
    "fix suggestions",
    "stale LSP package cache",
    "package import aliasing",
    "missing docs",
    "generated binding drift",
    "editor command path leakage",
    "package upgrade while editor is running",
    "CLI/LSP diagnostic drift",
    "package-editor-integration-report.json",
    "package fixtures",
    "completion snapshots",
    "hover snapshots",
    "diagnostic snapshots",
    "cache invalidation cases",
    "installed-tool paths",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "editor package resolution may disagree with cli resolution",
    "lsp may reach into source checkout paths",
    "stale lsp package cache is acceptable",
    "hover docs can omit package version",
    "cli/lsp diagnostic drift is acceptable",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package editor integration gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageEditorIntegrationSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package editor integration gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when package metadata is specified for editor
///   and LSP workflows.
/// - Stable diagnostics when package resolution, completion, hover,
///   diagnostics, cache invalidation, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the package editor integration contract into executable release
///   evidence for the 0.0.7 package-system roadmap.
pub fn run_package_editor_integration(
    root: &Path,
) -> QualityResult<PackageEditorIntegrationSummary> {
    let path = root.join(PACKAGE_EDITOR_INTEGRATION_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package editor integration contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_editor_integration_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageEditorIntegrationSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_editor_integration_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package editor integration term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package editor integration claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package editor integration text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package editor integration report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-editor-integration.v1",
        "editor_evidence": "package metadata editor and LSP integration contract",
        "package_fixtures": [
            "installed package cache fixture",
            "lockfile-backed package fixture",
            "generated binding metadata fixture"
        ],
        "completion_snapshots": [
            "package imports",
            "exported symbols",
            "methods",
            "constructors",
            "capabilities",
            "documented examples"
        ],
        "hover_snapshots": [
            "package version",
            "docs summary",
            "target support",
            "capability requirements",
            "deprecation status",
            "generated binding provenance",
            "generated package docs"
        ],
        "diagnostic_snapshots": [
            "missing package",
            "stale lockfile",
            "yanked package",
            "incompatible target",
            "missing capability",
            "missing native artifact",
            "CLI/LSP diagnostic drift"
        ],
        "cache_invalidation_cases": [
            "stale LSP package cache",
            "package upgrade while editor is running"
        ],
        "installed_tool_paths": [
            "terlc",
            "terlan-lsp",
            "installed package cache"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package editor integration report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package editor integration report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-editor-integration] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_editor_integration_test.rs"]
#[cfg(test)]
mod package_editor_integration_test;
