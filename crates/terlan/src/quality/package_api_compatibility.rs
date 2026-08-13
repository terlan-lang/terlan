use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_API_COMPATIBILITY_DOC: &str = "docs/package/TERLAN_PACKAGE_API_COMPATIBILITY.md";
const REPORT_PATH: &str = "target/quality/package-api-compatibility-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "public API manifest",
    "package names",
    "modules",
    "exported functions",
    "types",
    "constructors",
    "shapes",
    "capabilities",
    "generated bindings",
    "docs anchors",
    "examples",
    "diagnostics",
    "target support",
    "previous published package version",
    "diff classification",
    "additive",
    "compatible tightening",
    "deprecated",
    "breaking",
    "private",
    "target-only",
    "generated-binding-only",
    "semantic version policy",
    "patch releases cannot remove public APIs",
    "patch releases cannot break public APIs",
    "minor releases document additive surfaces",
    "major/pre-1 compatibility annotation",
    "migration guidance",
    "imports",
    "symbols",
    "removed exports without version bump",
    "changed function arity",
    "changed type shape",
    "stale docs anchors",
    "target support drift",
    "generated binding drift",
    "capability drift",
    "package examples importing removed APIs",
    "package-api-compatibility-report.json",
    "old manifest hashes",
    "new manifest hashes",
    "required version bump",
    "migration coverage",
    "rejected unclassified changes",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "api changes can skip classification",
    "patch releases may remove public APIs",
    "generated binding drift can skip the manifest",
    "capability drift is private",
    "migration guidance is optional for breaking changes",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package API compatibility gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageApiCompatibilitySummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package API compatibility gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when package API compatibility dimensions are
///   documented.
/// - Stable diagnostics when manifest fields, diff classifications, semver
///   policy, migration coverage, or adversarial API drift cases are missing.
///
/// Transformation:
/// - Converts the package API compatibility contract into executable release
///   evidence for the 0.0.7 package-system roadmap.
pub fn run_package_api_compatibility(root: &Path) -> QualityResult<PackageApiCompatibilitySummary> {
    let path = root.join(PACKAGE_API_COMPATIBILITY_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package API compatibility contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_api_compatibility_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageApiCompatibilitySummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_api_compatibility_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package API compatibility term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package API compatibility claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package API compatibility text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package API compatibility report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-api-compatibility.v1",
        "api_evidence": "public package API compatibility contract",
        "manifest_fields": [
            "package names",
            "modules",
            "exported functions",
            "types",
            "constructors",
            "shapes",
            "capabilities",
            "generated bindings",
            "docs anchors",
            "examples",
            "diagnostics",
            "target support"
        ],
        "diff_classifications": [
            "additive",
            "compatible tightening",
            "deprecated",
            "breaking",
            "private",
            "target-only",
            "generated-binding-only"
        ],
        "semantic_version_policy": [
            "patch releases cannot remove public APIs",
            "patch releases cannot break public APIs",
            "minor releases document additive surfaces",
            "breaking changes require major/pre-1 compatibility annotation",
            "breaking changes require migration guidance"
        ],
        "adversarial_changes": [
            "removed exports without version bump",
            "changed function arity",
            "changed type shape",
            "stale docs anchors",
            "target support drift",
            "generated binding drift",
            "capability drift",
            "package examples importing removed APIs"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package API compatibility report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package API compatibility report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-api-compatibility] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_api_compatibility_test.rs"]
#[cfg(test)]
mod package_api_compatibility_test;
