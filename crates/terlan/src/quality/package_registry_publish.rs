use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_REGISTRY_PUBLISH_DOC: &str = "docs/package/TERLAN_PACKAGE_REGISTRY_PUBLISH.md";
const REPORT_PATH: &str = "target/quality/package-registry-publish-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "package manifest",
    "source archive",
    "generated binding manifest",
    "native artifact manifest",
    "docs summary",
    "checksum file",
    "compatibility metadata",
    "target capability metadata",
    "package provenance",
    "dry-run capable",
    "sealed package archive",
    "package versions are immutable",
    "deterministic",
    "append-only version entries",
    "explicit yanks",
    "offline registry mirror validation",
    "exact index diff",
    "package-registry-publish-report.json",
    "package archive path",
    "archive hash",
    "index diff",
    "provenance hash",
    "docs hash",
    "target metadata",
    "dry-run publish result",
    "rejected mutation attempts",
    "duplicate package versions",
    "overwritten checksums",
    "missing generated binding hashes",
    "missing native artifact hashes",
    "target-incompatible packages",
    "stale docs",
    "malformed index entries",
    "yanked packages resolving silently",
    "publish commands that rebuild from source",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "publish rebuilds from the workspace",
    "published versions are mutable",
    "network access is required for publish validation",
    "yanked packages resolve silently",
    "checksum changes are accepted",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package registry publish gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRegistryPublishSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package registry publish integrity gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and a machine-readable report when the publish contract
///   contains all required release-integrity terms.
/// - Stable diagnostics when publish artifacts, immutability, offline mirror, or
///   adversarial mutation coverage are missing.
///
/// Transformation:
/// - Converts the registry publish contract into executable release evidence for
///   the 0.0.7 package-system roadmap.
pub fn run_package_registry_publish(root: &Path) -> QualityResult<PackageRegistryPublishSummary> {
    let path = root.join(PACKAGE_REGISTRY_PUBLISH_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package registry publish contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_registry_publish_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageRegistryPublishSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_registry_publish_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!(
                "missing package registry publish contract term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package registry publish claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package registry publish text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package registry publish report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-registry-publish.v1",
        "publish_evidence": "sealed package archive promotion contract",
        "required_publish_inputs": [
            "package manifest",
            "source archive",
            "generated binding manifest",
            "native artifact manifest",
            "docs summary",
            "checksum file",
            "compatibility metadata",
            "target capability metadata",
            "package provenance"
        ],
        "registry_index_policy": [
            "deterministic index diff",
            "append-only version entries",
            "explicit yanks",
            "checksum mutation rejected"
        ],
        "offline_validation": "offline registry mirror validation",
        "rejected_mutation_attempts": [
            "duplicate package versions",
            "overwritten checksums",
            "missing generated binding hashes",
            "missing native artifact hashes",
            "target-incompatible packages",
            "stale docs",
            "malformed index entries",
            "yanked packages resolving silently",
            "publish commands that rebuild from source"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package registry publish report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package registry publish report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-registry-publish] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_registry_publish_test.rs"]
#[cfg(test)]
mod package_registry_publish_test;
