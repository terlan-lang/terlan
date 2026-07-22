use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the release flake detection gate writes the roadmap-required
/// report.
#[test]
fn release_flake_detection_writes_report() {
    let repo = TempRepo::new("release_flake_detection_writes_report");
    repo.write(
        "docs/release/RELEASE_FLAKE_DETECTION.md",
        &complete_contract(),
    );

    let summary = run_release_flake_detection(repo.path()).expect("release flake detection gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/release-flake-detection-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/release-flake-detection-report.json"),
    )
    .expect("read release flake detection report");
    assert!(report.contains("terlan.release-flake-detection.v1"));
    assert!(report.contains("release flake detection and quarantine policy contract"));
    assert!(report.contains("quarantine_records"));
}

/// Verifies unclassified nondeterministic failure claims are rejected.
#[test]
fn release_flake_detection_rejects_unclassified_flake_claims() {
    let text = format!(
        "{}\ntest or gate can fail nondeterministically without a classified flake record",
        complete_contract()
    );

    let diagnostics = validate_release_flake_detection_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("classified flake record")),
        "diagnostics should reject unclassified flake claims: {diagnostics:?}"
    );
}

/// Verifies quarantine record evidence is required.
#[test]
fn release_flake_detection_rejects_missing_quarantine_records() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "quarantine records")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_release_flake_detection_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("quarantine records")),
        "diagnostics should reject missing quarantine records: {diagnostics:?}"
    );
}

/// Verifies placeholder flake policy text is rejected.
#[test]
fn release_flake_detection_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define flake policy later", complete_contract());

    let diagnostics = validate_release_flake_detection_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder release flake detection text")),
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
