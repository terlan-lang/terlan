use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_BUILD_ARTIFACT_ISOLATION_DOC: &str =
    "docs/package/TERLAN_PACKAGE_BUILD_ARTIFACT_ISOLATION.md";
const REPORT_PATH: &str = "target/quality/package-build-artifact-isolation-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "package",
    "workspace",
    "package identity",
    "version/source hash",
    "target",
    "capability set",
    "Stale package artifacts",
    "build",
    "test",
    "docs",
    "editor",
    "package CLI",
    "VM runtime",
    "package/workspace build artifact layout",
    "compiled modules",
    "VM artifacts",
    "generated docs",
    "generated bindings",
    "native artifacts",
    "test binaries",
    "diagnostics snapshots",
    "per-package caches",
    "Incremental builds",
    "source hash",
    "package manifest hash",
    "lockfile hash",
    "target/capability hash",
    "stdlib hash",
    "compiler version",
    "generated binding hash",
    "native artifact hash",
    "environment/config inputs",
    "namespaced",
    "Clean/check behavior",
    "package artifact directories",
    "workspace artifact directories",
    "dry-run output",
    "source",
    "lockfiles",
    "package caches",
    "live registry mirrors",
    "stale module output",
    "stale generated binding output",
    "changed stdlib hash",
    "changed compiler version",
    "target drift",
    "package rename collisions",
    "concurrent builds",
    "partial failed builds",
    "clean commands deleting the wrong package artifacts",
    "package-build-artifact-isolation-report.json",
    "artifact roots",
    "invalidation matrix",
    "stale-artifact fixtures",
    "clean dry-run output",
    "concurrency result",
    "diagnostics",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "stale package artifacts may affect build results",
    "artifact invalidation may differ between single-package and workspace builds",
    "clean commands may remove non-artifact state",
    "local packages can consume stale build output",
    "dry-run output is optional",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package build artifact isolation gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageBuildArtifactIsolationSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package build artifact isolation gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when package artifact isolation and incremental
///   invalidation semantics are documented.
/// - Stable diagnostics when layout, cache invalidation, namespace boundaries,
///   clean/check behavior, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the package build artifact isolation contract into executable
///   release evidence for the 0.0.7 package-system roadmap.
pub fn run_package_build_artifact_isolation(
    root: &Path,
) -> QualityResult<PackageBuildArtifactIsolationSummary> {
    let path = root.join(PACKAGE_BUILD_ARTIFACT_ISOLATION_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package build artifact isolation contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_build_artifact_isolation_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageBuildArtifactIsolationSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_build_artifact_isolation_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!(
                "missing package build artifact isolation term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package build artifact isolation claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package build artifact isolation text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package build artifact isolation report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-build-artifact-isolation.v1",
        "artifact_evidence": "package build artifact isolation contract",
        "artifact_roots": [
            "compiled modules",
            "VM artifacts",
            "generated docs",
            "generated bindings",
            "native artifacts",
            "test binaries",
            "diagnostics snapshots",
            "per-package caches"
        ],
        "invalidation_matrix": [
            "source hash",
            "package manifest hash",
            "lockfile hash",
            "target/capability hash",
            "stdlib hash",
            "compiler version",
            "generated binding hash",
            "native artifact hash",
            "environment/config inputs"
        ],
        "stale_artifact_fixtures": [
            "stale module output",
            "stale generated binding output",
            "changed stdlib hash",
            "changed compiler version",
            "target drift",
            "package rename collisions"
        ],
        "clean_dry_run_output": [
            "package artifact directories",
            "workspace artifact directories",
            "source protection",
            "lockfile protection",
            "package cache protection",
            "live registry mirror protection"
        ],
        "concurrency_result": [
            "concurrent builds",
            "partial failed builds"
        ],
        "diagnostics": [
            "clean commands deleting the wrong package artifacts",
            "artifact invalidation differs between single-package and workspace builds",
            "clean commands remove non-artifact state"
        ]
    });
    let text = serde_json::to_string_pretty(&report).map_err(|err| {
        format!("failed to serialize package build artifact isolation report: {err}")
    })?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package build artifact isolation report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-build-artifact-isolation] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_build_artifact_isolation_test.rs"]
mod package_build_artifact_isolation_test;
