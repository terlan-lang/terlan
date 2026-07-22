use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package workspace graph gate writes the roadmap-required report.
#[test]
fn package_workspace_graph_writes_report() {
    let repo = TempRepo::new("package_workspace_graph_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_WORKSPACE_GRAPH.md",
        &complete_contract(),
    );

    let summary = run_package_workspace_graph(repo.path()).expect("package workspace graph gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-workspace-graph-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-workspace-graph-report.json"),
    )
    .expect("read package workspace graph report");
    assert!(report.contains("terlan.package-workspace-graph.v1"));
    assert!(report.contains("deterministic multi-package workspace graph contract"));
    assert!(report.contains("cross-package generated binding drift"));
}

/// Verifies ambient registry nondeterminism claims are rejected.
#[test]
fn package_workspace_graph_rejects_ambient_registry_claims() {
    let text = format!(
        "{}\nambient registry state may affect workspace behavior",
        complete_contract()
    );

    let diagnostics = validate_package_workspace_graph_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("ambient registry state")),
        "diagnostics should reject ambient registry state claims: {diagnostics:?}"
    );
}

/// Verifies artifact isolation evidence is required.
#[test]
fn package_workspace_graph_rejects_missing_artifact_isolation() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "artifact isolation checks")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_workspace_graph_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("artifact isolation checks")),
        "diagnostics should reject missing artifact isolation evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder workspace graph contracts are rejected.
#[test]
fn package_workspace_graph_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: define workspace graph ordering later",
        complete_contract()
    );

    let diagnostics = validate_package_workspace_graph_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package workspace graph text")),
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
