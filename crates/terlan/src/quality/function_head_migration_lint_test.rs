use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn valid_parser_source() -> String {
    REQUIRED_PARSER_TERMS.join("\n")
}

fn valid_parser_tests() -> String {
    [
        "rejects_reverse_alias_function_head_pattern_parameter",
        "rejects_reverse_alias_clause_style_function_head_pattern_parameter",
        "build_command_rejects_function_head_pattern_for_js_target",
        "function_head_migration_diagnostic_policy_rejects_missing_matrix_columns",
    ]
    .join("\n")
}

fn valid_docs() -> String {
    MIGRATION_ROWS
        .iter()
        .map(|row| {
            format!(
                "## {}\n{}\n{}\n{}",
                row.id,
                row.doc_anchor.rsplit('#').next().unwrap_or(""),
                row.source_shape,
                row.suggested_rewrite
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn valid_makefile() -> String {
    REQUIRED_MAKE_TARGETS.join("\n")
}

/// Verifies the migration lint gate writes a reusable manifest.
#[test]
fn function_head_migration_lint_writes_manifest() {
    let repo = TempRepo::new("function_head_migration_lint_writes_manifest");
    repo.write(PARSER_SOURCE, &valid_parser_source());
    repo.write(PARSER_TEST_SOURCE, &valid_parser_tests());
    repo.write(DIAGNOSTIC_POLICY_TEST_SOURCE, &valid_parser_tests());
    repo.write(JS_TARGET_DIAGNOSTIC_TEST_SOURCE, &valid_parser_tests());
    repo.write(DOC_SOURCE, &valid_docs());
    repo.write(MAKEFILE, &valid_makefile());

    let summary =
        run_function_head_migration_lint(repo.path()).expect("function-head migration lint");

    assert_eq!(MIGRATION_ROWS.len(), summary.migration_row_count);
    assert_eq!(REQUIRED_PARSER_TERMS.len(), summary.parser_anchor_count);
    assert_eq!(REQUIRED_MAKE_TARGETS.len(), summary.make_target_count);

    let manifest = fs::read_to_string(repo.path().join(&summary.manifest_path))
        .expect("read migration manifest");
    assert!(manifest.contains("terlan.function-head-pattern-migration-manifest.v1"));
    assert!(manifest.contains("migration.function_head_pattern.invalid_alias_style"));
    assert!(manifest.contains("no_silent_rewrite"));
}

/// Verifies reverse-alias diagnostics cannot lose the migration ID.
#[test]
fn function_head_migration_lint_rejects_missing_parser_migration_id() {
    let parser = valid_parser_source().replace(
        "migration.function_head_pattern.invalid_alias_style",
        "syntax_error",
    );

    let diagnostics = validate_function_head_migration_lint_inputs(
        &parser,
        &valid_parser_tests(),
        &valid_docs(),
        &valid_makefile(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("invalid_alias_style")),
        "diagnostics should reject missing parser migration ID: {diagnostics:?}"
    );
}

/// Verifies Make integration remains part of the slice contract.
#[test]
fn function_head_migration_lint_rejects_missing_make_target() {
    let makefile = "function-head-migration-diagnostic-policy-check\n";

    let diagnostics = validate_function_head_migration_lint_inputs(
        &valid_parser_source(),
        &valid_parser_tests(),
        &valid_docs(),
        makefile,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("function-head-migration-lint-check")),
        "diagnostics should reject missing Make target: {diagnostics:?}"
    );
}

/// Verifies docs anchors are mandatory for every migration row.
#[test]
fn function_head_migration_lint_rejects_missing_docs_heading() {
    let docs = valid_docs().replace("## migration.function_head_pattern.safe_reject", "");

    let diagnostics = validate_function_head_migration_lint_inputs(
        &valid_parser_source(),
        &valid_parser_tests(),
        &docs,
        &valid_makefile(),
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("safe_reject")),
        "diagnostics should reject missing docs heading: {diagnostics:?}"
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
