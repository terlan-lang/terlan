use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn valid_command() -> String {
    REQUIRED_TERMS.join("\n")
}

fn valid_tests() -> String {
    REQUIRED_TESTS.join("\n")
}

fn valid_main() -> String {
    [
        "terlc migrate pattern-head [--write] [--json]",
        "\"migrate\" => commands::migrate::run(cmd)",
    ]
    .join("\n")
}

fn valid_makefile() -> String {
    REQUIRED_MAKE_TARGETS.join("\n")
}

fn valid_manifest() -> String {
    "migration.function_head_pattern.invalid_alias_style".to_string()
}

/// Verifies the migration assist gate writes a stable report.
#[test]
fn function_head_pattern_migration_assist_writes_report() {
    let repo = TempRepo::new("function_head_pattern_migration_assist_writes_report");
    repo.write(COMMAND_SOURCE, &valid_command());
    repo.write(COMMAND_TEST_SOURCE, &valid_tests());
    repo.write(MAIN_SOURCE, &valid_main());
    repo.write(MAKEFILE, &valid_makefile());
    repo.write(MANIFEST_PATH, &valid_manifest());

    let summary = run_function_head_pattern_migration_assist(repo.path())
        .expect("function-head migration assist");

    assert_eq!(REQUIRED_TERMS.len(), summary.required_term_count);
    assert_eq!(REQUIRED_TESTS.len(), summary.required_test_count);
    assert_eq!(REQUIRED_MAKE_TARGETS.len(), summary.make_target_count);
    let report = fs::read_to_string(repo.path().join(REPORT_PATH)).expect("read report");
    assert!(report.contains("terlan.function-head-pattern-migration-assist-gate.v1"));
    assert!(report.contains("dry-run is default"));
}

/// Verifies the gate fails when command tests stop proving dry-run behavior.
#[test]
fn function_head_pattern_migration_assist_rejects_missing_dry_run_test() {
    let diagnostics = validate_function_head_pattern_migration_assist(
        &valid_command(),
        &valid_tests().replace(
            "pattern_head_migration_dry_run_reports_plan_without_writing",
            "",
        ),
        &valid_main(),
        &valid_makefile(),
        &valid_manifest(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("dry_run")),
        "expected missing dry-run diagnostic: {diagnostics:?}"
    );
}

/// Verifies the gate fails when Make integration is missing.
#[test]
fn function_head_pattern_migration_assist_rejects_missing_make_target() {
    let diagnostics = validate_function_head_pattern_migration_assist(
        &valid_command(),
        &valid_tests(),
        &valid_main(),
        &valid_makefile().replace("function-head-pattern-migration-assist-check", ""),
        &valid_manifest(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("function-head-pattern-migration-assist-check")),
        "expected missing Make target diagnostic: {diagnostics:?}"
    );
}

/// Verifies the gate fails when the assist no longer reuses the lint manifest ID.
#[test]
fn function_head_pattern_migration_assist_rejects_missing_manifest_id() {
    let diagnostics = validate_function_head_pattern_migration_assist(
        &valid_command(),
        &valid_tests(),
        &valid_main(),
        &valid_makefile(),
        "",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("invalid-alias migration ID")),
        "expected missing manifest diagnostic: {diagnostics:?}"
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
