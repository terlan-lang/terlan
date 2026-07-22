use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the release gate duration-budget gate writes the roadmap-required
/// report.
#[test]
fn release_gate_duration_budget_writes_report() {
    let repo = TempRepo::new("release_gate_duration_budget_writes_report");
    repo.write(
        "docs/release/RELEASE_GATE_DURATION_BUDGET.md",
        &complete_contract(),
    );

    let summary =
        run_release_gate_duration_budget(repo.path()).expect("release gate duration-budget gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/release-gate-duration-budget-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/release-gate-duration-budget-report.json"),
    )
    .expect("read release gate duration-budget report");
    assert!(report.contains("terlan.release-gate-duration-budget.v1"));
    assert!(report.contains("release gate duration-budget contract"));
    assert!(report.contains("baseline_deltas"));
}

/// Verifies console timing claims are rejected.
#[test]
fn release_gate_duration_budget_rejects_console_timing_claims() {
    let text = format!(
        "{}\nduration budgets use console timing",
        complete_contract()
    );

    let diagnostics = validate_release_gate_duration_budget_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("console timing")),
        "diagnostics should reject console timing claims: {diagnostics:?}"
    );
}

/// Verifies slow-test labels are required.
#[test]
fn release_gate_duration_budget_rejects_missing_slow_test_labels() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| !term.contains("slow-test labels"))
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_release_gate_duration_budget_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("slow-test labels")),
        "diagnostics should reject missing slow-test label evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder duration-budget text is rejected.
#[test]
fn release_gate_duration_budget_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: define duration budget later",
        complete_contract()
    );

    let diagnostics = validate_release_gate_duration_budget_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder release gate duration-budget text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, text: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
