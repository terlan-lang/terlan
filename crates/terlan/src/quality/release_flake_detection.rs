use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const RELEASE_FLAKE_DETECTION_DOC: &str = "docs/release/RELEASE_FLAKE_DETECTION.md";
const REPORT_PATH: &str = "target/quality/release-flake-detection-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "release flake detection and quarantine policy",
    "deterministic flake-detection policy",
    "release gates",
    "repeat counts",
    "timeout multipliers",
    "allowed nondeterminism",
    "random seeds",
    "temp path normalization",
    "clock isolation",
    "network/socket isolation rules",
    "nondeterministic failure",
    "fixed",
    "quarantined",
    "intentionally unstable",
    "owner",
    "expiry date",
    "linked issue",
    "affected gate",
    "explicit release impact",
    "quarantined tests",
    "visible in release output",
    "silently reduce coverage",
    "adversarial corpus coverage",
    "benchmark comparability",
    "VM semantics coverage",
    "package compatibility coverage",
    "randomized test order",
    "stale temp directories",
    "clock-dependent diagnostics",
    "port reuse",
    "race-prone watchers",
    "benchmark warmup variance",
    "file-system ordering",
    "support-bundle path leakage",
    "release-flake-detection-report.json",
    "repeated run summaries",
    "seeds",
    "failure signatures",
    "quarantine records",
    "expiry validation",
    "timeout classification",
    "release-blocking decisions",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "test or gate can fail nondeterministically without a classified flake record",
    "quarantine entries expire",
    "hide coverage loss",
    "mask VM/runtime semantic regressions",
    "dropping unstable cases",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the release flake detection gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFlakeDetectionSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the release flake detection and quarantine policy gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/release/`.
///
/// Output:
/// - Success summary and report when deterministic flake classification,
///   quarantine visibility, adversarial cases, and report fields are documented.
/// - Stable diagnostics when flake policy, quarantine ownership, coverage
///   protection, or adversarial cases are missing.
///
/// Transformation:
/// - Converts the release flake detection contract into executable release
///   evidence for the 0.0.7 release gate roadmap.
pub fn run_release_flake_detection(root: &Path) -> QualityResult<ReleaseFlakeDetectionSummary> {
    let path = root.join(RELEASE_FLAKE_DETECTION_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release flake detection contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_release_flake_detection_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseFlakeDetectionSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_release_flake_detection_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing release flake detection term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!("forbidden release flake detection claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder release flake detection text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release flake detection report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.release-flake-detection.v1",
        "artifact_evidence": "release flake detection and quarantine policy contract",
        "repeated_run_summaries": [
            "repeat counts",
            "timeout multipliers",
            "allowed nondeterminism"
        ],
        "seeds": [
            "random seeds"
        ],
        "failure_signatures": [
            "nondeterministic failure",
            "clock-dependent diagnostics",
            "port reuse",
            "race-prone watchers"
        ],
        "quarantine_records": [
            "owner",
            "expiry date",
            "linked issue",
            "affected gate",
            "explicit release impact"
        ],
        "expiry_validation": [
            "quarantined",
            "intentionally unstable"
        ],
        "timeout_classification": [
            "timeout multipliers",
            "network/socket isolation rules"
        ],
        "release_blocking_decisions": [
            "fixed",
            "quarantined",
            "intentionally unstable"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize release flake detection report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release flake detection report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-flake-detection] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_flake_detection_test.rs"]
mod release_flake_detection_test;
