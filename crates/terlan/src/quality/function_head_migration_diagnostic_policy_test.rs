use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn complete_contract() -> String {
    REQUIRED_TERMS.join("\n")
}

/// Verifies the function-head migration diagnostic policy gate writes the
/// roadmap-required report.
#[test]
fn function_head_migration_diagnostic_policy_writes_report() {
    let repo = TempRepo::new("function_head_migration_diagnostic_policy_writes_report");
    repo.write(
        "docs/language/FUNCTION_HEAD_MIGRATION_DIAGNOSTIC_POLICY.md",
        &complete_contract(),
    );

    let summary = run_function_head_migration_diagnostic_policy(repo.path())
        .expect("function-head migration diagnostic policy gate");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(FORBIDDEN_CLAIMS.len(), summary.forbidden_claim_count);
    assert_eq!(
        "target/quality/function-head-migration-diagnostic-policy-report.json",
        summary.report_path
    );
    let report = fs::read_to_string(
        repo.path()
            .join("target/quality/function-head-migration-diagnostic-policy-report.json"),
    )
    .expect("read function-head migration diagnostic policy report");
    assert!(report.contains("terlan.function-head-migration-diagnostic-policy.v1"));
    assert!(report.contains("function-head migration diagnostic policy"));
    assert!(report.contains("compatibility_matrix"));
}

/// Verifies numeric fallback diagnostic codes are rejected.
#[test]
fn function_head_migration_diagnostic_policy_rejects_numeric_fallback_claims() {
    let text = format!(
        "{}\nimplicit numeric fallback codes are allowed",
        complete_contract()
    );

    let diagnostics = validate_function_head_migration_diagnostic_policy_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("numeric fallback codes")),
        "diagnostics should reject numeric fallback claims: {diagnostics:?}"
    );
}

/// Verifies the compatibility matrix columns are required.
#[test]
fn function_head_migration_diagnostic_policy_rejects_missing_matrix_columns() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "js_reject")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_function_head_migration_diagnostic_policy_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("js_reject")),
        "diagnostics should reject missing compatibility matrix columns: {diagnostics:?}"
    );
}

/// Verifies placeholder policy text is rejected.
#[test]
fn function_head_migration_diagnostic_policy_rejects_placeholder_text() {
    let text = format!(
        "{}\nTODO: define migration policy later",
        complete_contract()
    );

    let diagnostics = validate_function_head_migration_diagnostic_policy_text(&text);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .contains("placeholder function-head migration diagnostic policy text")),
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
