use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package API compatibility gate writes the roadmap-required
/// report.
#[test]
fn package_api_compatibility_writes_report() {
    let repo = TempRepo::new("package_api_compatibility_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_API_COMPATIBILITY.md",
        &complete_contract(),
    );

    let summary = run_package_api_compatibility(repo.path()).expect("api compatibility gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-api-compatibility-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-api-compatibility-report.json"),
    )
    .expect("read API compatibility report");
    assert!(report.contains("terlan.package-api-compatibility.v1"));
    assert!(report.contains("public package API compatibility contract"));
    assert!(report.contains("generated-binding-only"));
}

/// Verifies unclassified API drift claims are rejected.
#[test]
fn package_api_compatibility_rejects_unclassified_api_drift_claims() {
    let text = format!(
        "{}\napi changes can skip classification",
        complete_contract()
    );

    let diagnostics = validate_package_api_compatibility_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("api changes can skip classification")),
        "diagnostics should reject unclassified API drift: {diagnostics:?}"
    );
}

/// Verifies semver policy terms are required.
#[test]
fn package_api_compatibility_rejects_missing_semver_policy() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "patch releases cannot break public APIs")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_api_compatibility_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("patch releases cannot break public APIs")),
        "diagnostics should reject missing semver policy: {diagnostics:?}"
    );
}

/// Verifies placeholder API compatibility contracts are rejected.
#[test]
fn package_api_compatibility_rejects_placeholder_text() {
    let text = format!("{}\nTODO: classify API changes later", complete_contract());

    let diagnostics = validate_package_api_compatibility_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package API compatibility text")),
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
