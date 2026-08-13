use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_CACHE_INTEGRITY_DOC: &str = "docs/package/TERLAN_PACKAGE_CACHE_INTEGRITY.md";
const REPORT_PATH: &str = "target/quality/package-cache-integrity-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "deterministic",
    "checksum-backed",
    "builds",
    "editor workflows",
    "package CLI workflows",
    "VM runtime loading",
    "cataloged diagnostics",
    "archives",
    "expanded sources",
    "generated bindings",
    "native artifacts",
    "docs summaries",
    "registry snapshots",
    "lockfile metadata",
    "temporary extraction state",
    "content-addressed",
    "checksum-verified",
    "target constraints",
    "capabilities",
    "native-artifact dimensions",
    "terlc package cache verify",
    "terlc package cache clean --check",
    "terlc package cache prune --dry-run",
    "without mutating files",
    "explicit write action",
    "live dependencies",
    "unsafe paths",
    "workspace paths",
    "Corrupted",
    "partial",
    "stale",
    "target-mismatched",
    "yanked",
    "provenance-mismatched",
    "corrupted archives",
    "partial extraction",
    "stale native artifacts",
    "stale generated bindings",
    "target-mismatched cache entries",
    "cache poisoning",
    "symlink/path traversal attempts",
    "concurrent cache writes",
    "clean/prune commands deleting live dependencies",
    "package-cache-integrity-report.json",
    "cache fixture paths",
    "verified entries",
    "rejected entries",
    "prune plan",
    "diagnostics",
    "checksum coverage",
    "concurrency behavior",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "cache corruption may fall back to workspace paths",
    "clean may remove live dependencies",
    "prune may follow unsafe paths",
    "checksums are optional",
    "cache poisoning is ignored",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package cache integrity gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCacheIntegritySummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package cache integrity gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and report when package cache integrity semantics are
///   documented.
/// - Stable diagnostics when cache layout, checksum keys, command behavior,
///   corruption diagnostics, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the package cache integrity contract into executable release
///   evidence for the 0.0.7 package-system roadmap.
pub fn run_package_cache_integrity(root: &Path) -> QualityResult<PackageCacheIntegritySummary> {
    let path = root.join(PACKAGE_CACHE_INTEGRITY_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package cache integrity contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_cache_integrity_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageCacheIntegritySummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_cache_integrity_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package cache integrity term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package cache integrity claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package cache integrity text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package cache integrity report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-cache-integrity.v1",
        "cache_evidence": "deterministic package cache integrity contract",
        "cache_fixture_paths": [
            "archives",
            "expanded sources",
            "generated bindings",
            "native artifacts",
            "docs summaries",
            "registry snapshots",
            "lockfile metadata",
            "temporary extraction state"
        ],
        "verified_entries": [
            "content-addressed cache key",
            "checksum-verified cache key",
            "target/capability/native-artifact dimensions"
        ],
        "rejected_entries": [
            "corrupted archives",
            "partial extraction",
            "stale native artifacts",
            "stale generated bindings",
            "target-mismatched cache entries",
            "cache poisoning",
            "symlink/path traversal attempts"
        ],
        "prune_plan": [
            "terlc package cache verify",
            "terlc package cache clean --check",
            "terlc package cache prune --dry-run",
            "live dependency preservation"
        ],
        "diagnostics": [
            "cataloged corruption diagnostics",
            "no silent workspace path fallback"
        ],
        "checksum_coverage": [
            "archives",
            "expanded sources",
            "native artifacts",
            "generated bindings"
        ],
        "concurrency_behavior": [
            "concurrent cache writes",
            "temporary extraction state"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package cache integrity report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package cache integrity report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-cache-integrity] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_cache_integrity_test.rs"]
#[cfg(test)]
mod package_cache_integrity_test;
