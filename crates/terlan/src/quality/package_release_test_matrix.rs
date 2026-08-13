use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_RELEASE_MATRIX_DOC: &str = "docs/package/TERLAN_PACKAGE_RELEASE_TEST_MATRIX.md";
const REPORT_PATH: &str = "target/quality/package-release-test-matrix-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "package type",
    "target support",
    "capability contract",
    "tests",
    "examples",
    "docs",
    "generated artifacts",
    "native artifacts",
    "publish readiness state",
    "clean temporary workspaces",
    "installed compiler",
    "installed stdlib",
    "package lockfile",
    "local registry mirror",
    "VM default runtime",
    "verified alternate artifact",
    "build",
    "test",
    "docs generation",
    "example execution",
    "formatter checks",
    "lint checks",
    "capability denial paths",
    "package resolver behavior",
    "lockfile behavior",
    "support-bundle output on failure",
    "binding generation",
    "native artifact discovery",
    "target compatibility diagnostics",
    "stale handle diagnostics",
    "cancellation behavior",
    "missing native dependency skips",
    "packages with no examples",
    "packages with docs that do not compile",
    "packages that pass only from workspace paths",
    "missing capability tests",
    "stale generated bindings",
    "broken lockfiles",
    "missing target metadata",
    "publish-ready packages without tests",
    "package-release-test-matrix-report.json",
    "package rows",
    "target rows",
    "command results",
    "docs/examples coverage",
    "capability coverage",
    "native coverage",
    "skipped rows",
    "publish readiness status",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "workspace paths are required",
    "ambient network access is allowed",
    "non-vm default runtime is allowed",
    "publish-ready packages may omit tests",
    "docs compile is optional",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package release test matrix gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReleaseTestMatrixSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package release test matrix gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when package matrix dimensions and adversarial
///   cases are documented.
/// - Stable diagnostics when release matrix coverage or deterministic execution
///   constraints are missing.
///
/// Transformation:
/// - Converts the package release matrix contract into executable release
///   evidence for the 0.0.7 package-system roadmap.
pub fn run_package_release_test_matrix(
    root: &Path,
) -> QualityResult<PackageReleaseTestMatrixSummary> {
    let path = root.join(PACKAGE_RELEASE_MATRIX_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package release test matrix contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_release_test_matrix_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageReleaseTestMatrixSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_release_test_matrix_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package release test matrix term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package release test matrix claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package release test matrix text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package release matrix report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-release-test-matrix.v1",
        "matrix_evidence": "first-party package release test matrix contract",
        "workspace_requirements": [
            "clean temporary workspaces",
            "installed compiler",
            "installed stdlib",
            "package lockfile",
            "local registry mirror",
            "VM default runtime"
        ],
        "command_results": [
            "build",
            "test",
            "docs generation",
            "example execution",
            "formatter checks",
            "lint checks",
            "capability denial paths",
            "package resolver behavior",
            "lockfile behavior",
            "support-bundle output on failure"
        ],
        "native_coverage": [
            "binding generation",
            "native artifact discovery",
            "target compatibility diagnostics",
            "stale handle diagnostics",
            "cancellation behavior",
            "missing native dependency skips"
        ],
        "adversarial_rows": [
            "packages with no examples",
            "packages with docs that do not compile",
            "packages that pass only from workspace paths",
            "missing capability tests",
            "stale generated bindings",
            "broken lockfiles",
            "missing target metadata",
            "publish-ready packages without tests"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package release matrix report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package release matrix report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-release-test-matrix] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_release_test_matrix_test.rs"]
#[cfg(test)]
mod package_release_test_matrix_test;
