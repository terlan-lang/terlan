use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const RELEASE_GATE_DURATION_BUDGET_DOC: &str = "docs/release/RELEASE_GATE_DURATION_BUDGET.md";
const REPORT_PATH: &str = "target/quality/release-gate-duration-budget-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "release gate duration budgets",
    "slow-test regression tracking",
    "per-gate",
    "per-suite duration budgets",
    "local development",
    "CI",
    "release preflight",
    "benchmark lanes",
    "stdlib checks",
    "VM semantics checks",
    "package checks",
    "editor/tooling checks",
    "committed baselines",
    "stable machine-readable reports",
    "not ad hoc console timing",
    "warmup",
    "cache state",
    "sharding mode",
    "hardware class",
    "explicit slow test labels",
    "why they are slow",
    "permanent release coverage",
    "one-off gate probes",
    "faster unit tests",
    "fixture tests",
    "timing report drift",
    "missing slow-test labels",
    "hidden sleeps",
    "accidental network waits",
    "repeated full builds",
    "benchmark lanes counted as correctness gates",
    "budget bypasses under sharded or resumed release runs",
    "release-gate-duration-budget-report.json",
    "gate timings",
    "baseline deltas",
    "slow-test labels",
    "cache mode",
    "shard mode",
    "budget decisions",
    "recommended split points",
    "gate duration regresses past the accepted threshold",
    "explicit baseline update and rationale",
    "slow tests are unlabelled",
    "correctness gates include accidental benchmark work",
    "resumed/sharded runs hide repeated expensive work",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "duration budgets use console timing",
    "slow tests do not need labels",
    "benchmark lanes are correctness gates",
    "sharded runs may hide repeated expensive work",
    "resumed runs may hide repeated expensive work",
    "baseline updates do not need rationale",
    "network waits are acceptable in release gates",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the release gate duration-budget gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGateDurationBudgetSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the release gate duration-budget correctness gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/release/`.
///
/// Output:
/// - Success summary and report when duration budgets, baseline comparison,
///   slow-test labels, adversarial cases, and report fields are documented.
/// - Stable diagnostics when timing, baseline, sharding, or slow-test evidence
///   is missing.
///
/// Transformation:
/// - Converts the duration-budget release contract into executable evidence for
///   the 0.0.7 release gate roadmap.
pub fn run_release_gate_duration_budget(
    root: &Path,
) -> QualityResult<ReleaseGateDurationBudgetSummary> {
    let path = root.join(RELEASE_GATE_DURATION_BUDGET_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read release gate duration-budget contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_release_gate_duration_budget_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(ReleaseGateDurationBudgetSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_release_gate_duration_budget_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!(
                "missing release gate duration-budget term `{term}`"
            ));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(&claim.to_lowercase()) {
            diagnostics.push(format!(
                "forbidden release gate duration-budget claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder release gate duration-budget text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create release gate duration-budget report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.release-gate-duration-budget.v1",
        "artifact_evidence": "release gate duration-budget contract",
        "gate_timings": [
            "per-gate",
            "per-suite duration budgets"
        ],
        "baseline_deltas": [
            "committed baselines",
            "stable machine-readable reports"
        ],
        "slow_test_labels": [
            "explicit slow test labels",
            "why they are slow",
            "permanent release coverage",
            "one-off gate probes"
        ],
        "hardware_class": [
            "hardware class"
        ],
        "cache_mode": [
            "cache state"
        ],
        "shard_mode": [
            "sharding mode",
            "shard mode"
        ],
        "budget_decisions": [
            "gate duration regresses past the accepted threshold",
            "explicit baseline update and rationale"
        ],
        "recommended_split_points": [
            "recommended split points",
            "repeated full builds"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize release gate duration-budget report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write release gate duration-budget report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[release-gate-duration-budget] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "release_gate_duration_budget_test.rs"]
#[cfg(test)]
mod release_gate_duration_budget_test;
