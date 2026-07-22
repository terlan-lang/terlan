use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package build artifact isolation gate writes the
/// roadmap-required report.
#[test]
fn package_build_artifact_isolation_writes_report() {
    let repo = TempRepo::new("package_build_artifact_isolation_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_BUILD_ARTIFACT_ISOLATION.md",
        &complete_contract(),
    );

    let summary = run_package_build_artifact_isolation(repo.path())
        .expect("package build artifact isolation gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-build-artifact-isolation-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-build-artifact-isolation-report.json"),
    )
    .expect("read package build artifact isolation report");
    assert!(report.contains("terlan.package-build-artifact-isolation.v1"));
    assert!(report.contains("package build artifact isolation contract"));
    assert!(report.contains("changed compiler version"));
}

/// Verifies stale artifact result claims are rejected.
#[test]
fn package_build_artifact_isolation_rejects_stale_artifact_claims() {
    let text = format!(
        "{}\nstale package artifacts may affect build results",
        complete_contract()
    );

    let diagnostics = validate_package_build_artifact_isolation_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("stale package artifacts")),
        "diagnostics should reject stale artifact result claims: {diagnostics:?}"
    );
}

/// Verifies invalidation evidence is required.
#[test]
fn package_build_artifact_isolation_rejects_missing_invalidation_matrix() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "invalidation matrix")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_build_artifact_isolation_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("invalidation matrix")),
        "diagnostics should reject missing invalidation matrix evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder artifact isolation contracts are rejected.
#[test]
fn package_build_artifact_isolation_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: define artifact invalidation later",
        complete_contract()
    );

    let diagnostics = validate_package_build_artifact_isolation_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .contains("placeholder package build artifact isolation text")),
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
