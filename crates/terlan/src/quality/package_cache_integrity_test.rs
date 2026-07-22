use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the package cache integrity gate writes the roadmap-required report.
#[test]
fn package_cache_integrity_writes_report() {
    let repo = TempRepo::new("package_cache_integrity_writes_report");
    repo.write(
        "docs/package/TERLAN_PACKAGE_CACHE_INTEGRITY.md",
        &complete_contract(),
    );

    let summary = run_package_cache_integrity(repo.path()).expect("package cache integrity gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/package-cache-integrity-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/package-cache-integrity-report.json"),
    )
    .expect("read package cache integrity report");
    assert!(report.contains("terlan.package-cache-integrity.v1"));
    assert!(report.contains("deterministic package cache integrity contract"));
    assert!(report.contains("live dependency preservation"));
}

/// Verifies unsafe workspace fallback claims are rejected.
#[test]
fn package_cache_integrity_rejects_workspace_fallback_claims() {
    let text = format!(
        "{}\ncache corruption may fall back to workspace paths",
        complete_contract()
    );

    let diagnostics = validate_package_cache_integrity_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("workspace paths")),
        "diagnostics should reject workspace fallback claims: {diagnostics:?}"
    );
}

/// Verifies checksum coverage is required.
#[test]
fn package_cache_integrity_rejects_missing_checksum_coverage() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "checksum coverage")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_package_cache_integrity_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("checksum coverage")),
        "diagnostics should reject missing checksum coverage: {diagnostics:?}"
    );
}

/// Verifies placeholder cache integrity contracts are rejected.
#[test]
fn package_cache_integrity_rejects_placeholder_text() {
    let text = format!("{}\nTODO: define cache pruning later", complete_contract());

    let diagnostics = validate_package_cache_integrity_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package cache integrity text")),
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
