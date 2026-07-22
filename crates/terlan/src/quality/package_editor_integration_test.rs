use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package editor integration gate writes the roadmap-required
/// report.
#[test]
fn package_editor_integration_writes_report() {
    let repo = TempRepo::new("package_editor_integration_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_EDITOR_INTEGRATION.md",
        &complete_contract(),
    );

    let summary =
        run_package_editor_integration(repo.path()).expect("package editor integration gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-editor-integration-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-editor-integration-report.json"),
    )
    .expect("read package editor integration report");
    assert!(report.contains("terlan.package-editor-integration.v1"));
    assert!(report.contains("package metadata editor and LSP integration contract"));
    assert!(report.contains("CLI/LSP diagnostic drift"));
}

/// Verifies source checkout path fallbacks are rejected.
#[test]
fn package_editor_integration_rejects_source_checkout_path_fallbacks() {
    let text = format!(
        "{}\nlsp may reach into source checkout paths",
        complete_contract()
    );

    let diagnostics = validate_package_editor_integration_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("source checkout paths")),
        "diagnostics should reject source path fallbacks: {diagnostics:?}"
    );
}

/// Verifies hover package-version evidence is required.
#[test]
fn package_editor_integration_rejects_missing_hover_version() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "package version")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_editor_integration_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("package version")),
        "diagnostics should reject missing hover package-version evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder editor integration contracts are rejected.
#[test]
fn package_editor_integration_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: add editor package tests later",
        complete_contract()
    );

    let diagnostics = validate_package_editor_integration_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package editor integration text")),
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
