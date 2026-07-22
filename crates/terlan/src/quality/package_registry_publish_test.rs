use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package registry publish gate writes the roadmap-required
/// machine-readable report.
#[test]
fn package_registry_publish_writes_report() {
    let repo = TempRepo::new("package_registry_publish_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_REGISTRY_PUBLISH.md",
        &complete_contract(),
    );

    let summary = run_package_registry_publish(repo.path()).expect("registry publish gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-registry-publish-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-registry-publish-report.json"),
    )
    .expect("read publish report");
    assert!(report.contains("terlan.package-registry-publish.v1"));
    assert!(report.contains("sealed package archive promotion contract"));
    assert!(report.contains("publish commands that rebuild from source"));
}

/// Verifies mutable publish claims are rejected.
#[test]
fn package_registry_publish_rejects_forbidden_claims() {
    let text = format!("{}\npublished versions are mutable", complete_contract());

    let diagnostics = validate_package_registry_publish_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("published versions are mutable")),
        "diagnostics should reject mutable package version claims: {diagnostics:?}"
    );
}

/// Verifies report fields are enforced.
#[test]
fn package_registry_publish_rejects_missing_report_field() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "rejected mutation attempts")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_registry_publish_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("rejected mutation attempts")),
        "diagnostics should reject missing report field: {diagnostics:?}"
    );
}

/// Verifies placeholder publish contracts are rejected.
#[test]
fn package_registry_publish_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: fill in publish inputs later",
        complete_contract()
    );

    let diagnostics = validate_package_registry_publish_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package registry publish text")),
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
