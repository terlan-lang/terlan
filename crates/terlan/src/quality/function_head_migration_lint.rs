use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PARSER_SOURCE: &str = "crates/terlan/src/compiler/syntax/parser/callables.rs";
const PARSER_TEST_SOURCE: &str = "crates/terlan/src/compiler/syntax/parser_decl_test.rs";
const DIAGNOSTIC_POLICY_TEST_SOURCE: &str =
    "crates/terlan/src/quality/function_head_migration_diagnostic_policy_test.rs";
const JS_TARGET_DIAGNOSTIC_TEST_SOURCE: &str =
    "crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs";
const EXECUTABLE_ANCHOR_SOURCES: &[&str] = &[
    PARSER_TEST_SOURCE,
    DIAGNOSTIC_POLICY_TEST_SOURCE,
    JS_TARGET_DIAGNOSTIC_TEST_SOURCE,
];
const DOC_SOURCE: &str = "docs/language/function_heads.md";
const MAKEFILE: &str = "Makefile";
const MANIFEST_PATH: &str = "target/quality/function-head-pattern-migration-manifest.json";

const MIGRATION_ROWS: &[MigrationRow] = &[
    MigrationRow {
        id: "migration.function_head_pattern.invalid_alias_style",
        family: "syntax_error",
        severity: "error",
        source_shape: "alias = pattern: Type",
        suggested_rewrite: "{pattern} = alias: Type",
        doc_anchor:
            "docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style",
        executable_anchor: "rejects_reverse_alias_function_head_pattern_parameter",
    },
    MigrationRow {
        id: "migration.function_head_pattern.safe_reject",
        family: "migration_help",
        severity: "warning_or_error_in_strict_mode",
        source_shape: "ambiguous pattern-head rewrite",
        suggested_rewrite: "leave source unchanged and explain unsafe rewrite",
        doc_anchor: "docs/language/function_heads.md#migrationfunction_head_patternsafe_reject",
        executable_anchor:
            "function_head_migration_diagnostic_policy_rejects_missing_matrix_columns",
    },
    MigrationRow {
        id: "migration.function_head_pattern.unsupported_backend",
        family: "target_profile_unsupported",
        severity: "error",
        source_shape: "VM-only function-head pattern lowered to unsupported target",
        suggested_rewrite: "select VM target or avoid target-specific pattern head",
        doc_anchor:
            "docs/language/function_heads.md#migrationfunction_head_patternunsupported_backend",
        executable_anchor: "build_command_rejects_function_head_pattern_for_js_target",
    },
];

const REQUIRED_MAKE_TARGETS: &[&str] = &[
    "function-head-migration-lint-check",
    "function-head-migration-diagnostic-policy-check",
    "function-head-pattern-parameters-hardening-check",
];

const REQUIRED_PARSER_TERMS: &[&str] = &[
    "migration.function_head_pattern.invalid_alias_style",
    "{pattern} = name: Type",
    "docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style",
    "reverse_alias_parameter_starts",
];

/// One migration-lint row emitted into the function-head migration manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MigrationRow {
    id: &'static str,
    family: &'static str,
    severity: &'static str,
    source_shape: &'static str,
    suggested_rewrite: &'static str,
    doc_anchor: &'static str,
    executable_anchor: &'static str,
}

/// Summary produced by the function-head migration lint gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHeadMigrationLintSummary {
    pub migration_row_count: usize,
    pub parser_anchor_count: usize,
    pub make_target_count: usize,
    pub manifest_path: String,
}

/// Runs the function-head pattern migration lint gate.
///
/// Inputs:
/// - `root`: repository root containing parser, docs, Makefile, and tests.
///
/// Output:
/// - Success summary and a generated migration manifest when parser diagnostics,
///   docs anchors, executable anchors, and Make wiring agree.
/// - Stable diagnostics when a migration row cannot be proven by source.
///
/// Transformation:
/// - Turns the Slice 8 migration lint contract into a checked manifest that
///   editor, CLI, and future codemod work can consume without guessing IDs.
pub fn run_function_head_migration_lint(
    root: &Path,
) -> QualityResult<FunctionHeadMigrationLintSummary> {
    let parser = read_required_file(root, PARSER_SOURCE)?;
    let parser_tests = read_executable_anchor_sources(root)?;
    let docs = read_required_file(root, DOC_SOURCE)?;
    let makefile = read_required_file(root, MAKEFILE)?;

    let diagnostics =
        validate_function_head_migration_lint_inputs(&parser, &parser_tests, &docs, &makefile);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let manifest_path = root.join(MANIFEST_PATH);
    write_manifest(&manifest_path)?;

    Ok(FunctionHeadMigrationLintSummary {
        migration_row_count: MIGRATION_ROWS.len(),
        parser_anchor_count: REQUIRED_PARSER_TERMS.len(),
        make_target_count: REQUIRED_MAKE_TARGETS.len(),
        manifest_path: MANIFEST_PATH.to_string(),
    })
}

fn read_required_file(root: &Path, relative_path: &str) -> QualityResult<String> {
    let path = root.join(relative_path);
    fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read function-head migration lint input: {err}",
            path.display()
        )
    })
}

fn read_executable_anchor_sources(root: &Path) -> QualityResult<String> {
    let mut combined = String::new();
    for source in EXECUTABLE_ANCHOR_SOURCES {
        combined.push_str(&read_required_file(root, source)?);
        combined.push('\n');
    }
    Ok(combined)
}

fn validate_function_head_migration_lint_inputs(
    parser: &str,
    parser_tests: &str,
    docs: &str,
    makefile: &str,
) -> Vec<String> {
    let mut diagnostics = Vec::new();

    for term in REQUIRED_PARSER_TERMS {
        if !parser.contains(term) {
            diagnostics.push(format!("parser is missing migration lint term `{term}`"));
        }
    }

    for row in MIGRATION_ROWS {
        if !docs.contains(row.id) {
            diagnostics.push(format!("docs are missing migration row `{}`", row.id));
        }
        let heading = format!("## {}", row.id);
        if !docs.contains(&heading) {
            diagnostics.push(format!("docs are missing migration heading `{heading}`"));
        }
        if !docs.contains(row.doc_anchor.rsplit('#').next().unwrap_or(row.doc_anchor)) {
            diagnostics.push(format!(
                "docs are missing migration anchor fragment for `{}`",
                row.id
            ));
        }
        if !parser_tests.contains(row.executable_anchor) && !parser.contains(row.executable_anchor)
        {
            diagnostics.push(format!(
                "source is missing executable migration anchor `{}`",
                row.executable_anchor
            ));
        }
    }

    for anchor in [
        "rejects_reverse_alias_function_head_pattern_parameter",
        "rejects_reverse_alias_clause_style_function_head_pattern_parameter",
    ] {
        if !parser_tests.contains(anchor) {
            diagnostics.push(format!(
                "parser tests are missing reverse-alias anchor `{anchor}`"
            ));
        }
    }

    for target in REQUIRED_MAKE_TARGETS {
        if !makefile.contains(target) {
            diagnostics.push(format!(
                "Makefile is missing migration lint target `{target}`"
            ));
        }
    }

    diagnostics
}

fn write_manifest(manifest_path: &Path) -> QualityResult<()> {
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create function-head migration manifest directory: {err}",
                parent.display()
            )
        })?;
    }
    let rows = MIGRATION_ROWS
        .iter()
        .map(|row| {
            json!({
                "migration_id": row.id,
                "diagnostic_family": row.family,
                "severity": row.severity,
                "source_shape": row.source_shape,
                "suggested_rewrite": row.suggested_rewrite,
                "docs": row.doc_anchor,
                "executable_anchor": row.executable_anchor
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "schema": "terlan.function-head-pattern-migration-manifest.v1",
        "feature": "function-head pattern parameters",
        "strict_profile": {
            "warning_to_error": "same migration ID",
            "no_silent_rewrite": true
        },
        "rows": rows
    });
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to serialize function-head migration manifest: {err}"))?;
    fs::write(manifest_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write function-head migration manifest: {err}",
            manifest_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[function-head-migration-lint] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "function_head_migration_lint_test.rs"]
#[cfg(test)]
mod function_head_migration_lint_test;
