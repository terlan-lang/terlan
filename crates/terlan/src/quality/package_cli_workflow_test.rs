use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package CLI workflow gate writes the roadmap-required report.
#[test]
fn package_cli_workflow_writes_report() {
    let repo = TempRepo::new("package_cli_workflow_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_CLI_WORKFLOW.md",
        &complete_contract(),
    );

    let summary = run_package_cli_workflow(repo.path()).expect("package CLI workflow gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-cli-workflow-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-cli-workflow-report.json"),
    )
    .expect("read package CLI workflow report");
    assert!(report.contains("terlan.package-cli-workflow.v1"));
    assert!(report.contains("deterministic package CLI workflow contract"));
    assert!(report.contains("terlc package publish --dry-run"));
}

/// Verifies implicit package registry network claims are rejected.
#[test]
fn package_cli_workflow_rejects_implicit_network_claims() {
    let text = format!("{}\nnetwork access is implicit", complete_contract());

    let diagnostics = validate_package_cli_workflow_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("network access is implicit")),
        "diagnostics should reject implicit network claims: {diagnostics:?}"
    );
}

/// Verifies output evidence terms are required.
#[test]
fn package_cli_workflow_rejects_missing_output_snapshot() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "output snapshots")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_cli_workflow_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("output snapshots")),
        "diagnostics should reject missing output snapshot evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder package CLI workflow contracts are rejected.
#[test]
fn package_cli_workflow_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define cache behavior later", complete_contract());

    let diagnostics = validate_package_cli_workflow_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package CLI workflow text")),
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
