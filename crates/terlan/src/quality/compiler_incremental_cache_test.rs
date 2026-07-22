use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the compiler incremental cache gate writes the roadmap-required
/// report.
#[test]
fn compiler_incremental_cache_writes_report() {
    let repo = TempRepo::new("compiler_incremental_cache_writes_report");
    repo.write(
        "docs/compiler/COMPILER_INCREMENTAL_CACHE.md",
        &complete_contract(),
    );

    let summary =
        run_compiler_incremental_cache(repo.path()).expect("compiler incremental cache gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/compiler-incremental-cache-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/compiler-incremental-cache-report.json"),
    )
    .expect("read compiler incremental cache report");
    assert!(report.contains("terlan.compiler-incremental-cache.v1"));
    assert!(report.contains("compiler incremental cache correctness contract"));
    assert!(report.contains("clean_build_hashes"));
}

/// Verifies filesystem-order cache correctness claims are rejected.
#[test]
fn compiler_incremental_cache_rejects_filesystem_order_claims() {
    let text = format!(
        "{}\ncache correctness depends on filesystem order",
        complete_contract()
    );

    let diagnostics = validate_compiler_incremental_cache_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("filesystem order")),
        "diagnostics should reject filesystem-order cache claims: {diagnostics:?}"
    );
}

/// Verifies cache key evidence is required.
#[test]
fn compiler_incremental_cache_rejects_missing_cache_keys() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "compiler cache keys")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_compiler_incremental_cache_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("compiler cache keys")),
        "diagnostics should reject missing cache key evidence: {diagnostics:?}"
    );
}

/// Verifies placeholder incremental cache contracts are rejected.
#[test]
fn compiler_incremental_cache_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: define incremental cache later",
        complete_contract()
    );

    let diagnostics = validate_compiler_incremental_cache_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder compiler incremental cache text")),
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
