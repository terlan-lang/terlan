use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::{fix_semicolon_chains, lint_source, run, run_lint};
use crate::support::test_fs::{temp_dir, write_file};

#[path = "lint_test/actor_vm_test.rs"]
mod actor_vm_test;
#[path = "lint_test/complexity_test.rs"]
mod complexity_test;
#[path = "lint_test/consistency_test.rs"]
mod consistency_test;
#[path = "lint_test/generated_test.rs"]
mod generated_test;
#[path = "lint_test/imports_test.rs"]
mod imports_test;
#[path = "lint_test/maintainability_test.rs"]
mod maintainability_test;
#[path = "lint_test/naming_test.rs"]
mod naming_test;
#[path = "lint_test/pipe_test.rs"]
mod pipe_test;
#[path = "lint_test/readability_test.rs"]
mod readability_test;
#[path = "lint_test/targets_test.rs"]
mod targets_test;
#[path = "lint_test/test_rules_test.rs"]
mod test_rules_test;

/// Verifies dense semicolon chains emit stable lint diagnostics.
///
/// Inputs:
/// - A source module containing multiple same-line expressions.
///
/// Output:
/// - One diagnostic with stable rule identity, severity, location, and fix
///   availability.
///
/// Transformation:
/// - Runs the raw source lint pass directly so the rule contract is independent
///   from filesystem traversal.
#[test]
fn lint_reports_semicolon_chain_with_stable_diagnostic() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * Clears users and validates the result.
 */
pub clear(): Bool ->
    users.clear(); assert_equal(0, users.size()).
"#,
    );

    assert_eq!(diagnostics.len(), 1);
    let rendered = super::render_diagnostic(&diagnostics[0]);
    assert!(rendered.contains("Sample.terl:8:18"));
    assert!(rendered.contains("warning[TL0001:readability.semicolon-chain]"));
    assert!(rendered.contains("split dense semicolon expression chains"));
    assert!(rendered.ends_with("[fix available]"));
}

/// Verifies the safe fixer splits simple semicolon chains.
///
/// Inputs:
/// - A line with no strings or comments and multiple expression statements.
///
/// Output:
/// - One expression per line, preserving non-final semicolons.
///
/// Transformation:
/// - Applies the source-text fixer without parsing so the proof boundary stays
///   conservative and explicit.
#[test]
fn lint_fix_splits_simple_semicolon_chain() {
    let fixed = fix_semicolon_chains("    first(); second(); third().\n");

    assert_eq!(fixed, "    first();\n    second();\n    third().\n");
}

/// Verifies the fixer refuses comment-bearing semicolon lines.
#[test]
fn lint_fix_preserves_comment_semicolon_lines() {
    let source = "    one(); two(). // comment\n";

    assert_eq!(fix_semicolon_chains(source), source);
}

/// Verifies string contents do not prevent safe outer semicolon splitting.
#[test]
fn lint_fix_splits_semicolon_chains_with_string_arguments() {
    let fixed = fix_semicolon_chains(
        "    let users = Map({\"alice\", 1}); assert_equal(1, users.size()).\n",
    );

    assert_eq!(
        fixed,
        "    let users = Map({\"alice\", 1});\n    assert_equal(1, users.size()).\n"
    );
}

/// Verifies directory linting is deterministic and reports nested source files.
#[test]
fn lint_reports_directory_sources_in_sorted_order() {
    let root = temp_dir("lint_command", "directory_order");
    write_file(
        &root.join("b/B.terl"),
        "module b.B.\n/**\n * Runs B.\n */\npub b(): Unit ->\n    b1(); b2().\n",
    );
    write_file(
        &root.join("a/A.terl"),
        "module a.A.\n/**\n * Runs A.\n */\npub a(): Unit ->\n    a1(); a2().\n",
    );

    let diagnostics = run_lint(&root, false).expect("lint directory");

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics[0].path.ends_with("a/A.terl"));
    assert!(diagnostics[1].path.ends_with("b/B.terl"));
}

/// Verifies `terlc lint --fix` rewrites safe dense chains and exits cleanly.
#[test]
fn lint_command_fix_rewrites_safe_chain() {
    let root = temp_dir("lint_command", "fix");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        "module app.Main.\n/**\n * Runs the fixture.\n */\npub main(): Unit ->\n    first(); second(); third().\n",
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(fixed.contains("    first();\n    second();\n    third()."));
}

/// Verifies lint command diagnostics use a non-zero exit without mutating files.
#[test]
fn lint_command_reports_unfixed_chain() {
    let root = temp_dir("lint_command", "diagnostic_exit");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        "module app.Main.\n/**\n * Runs the fixture.\n */\npub main(): Unit ->\n    first(); second().\n",
    );

    assert_eq!(
        run(&[source_path.to_string_lossy().to_string()]),
        ExitCode::from(1)
    );

    let source = fs::read_to_string(source_path).expect("source after lint");
    assert!(source.contains("first(); second()."));
}

/// Verifies malformed lint options fail before filesystem access.
#[test]
fn lint_command_rejects_unknown_flag() {
    assert_eq!(
        run(&["--unsafe-fix".to_string(), "Main.terl".to_string()]),
        ExitCode::from(2)
    );
}

/// Verifies literal true assertions in test files emit the fake-test rule.
#[test]
fn lint_reports_literal_true_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub literal_true_assertion_is_fake(): Bool ->
    assert(true).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0401:tests.fake]"));
    assert!(rendered.contains("literal true assertion does not prove behavior"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies literal true test bodies emit the fake-test rule.
#[test]
fn lint_reports_literal_true_body_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub literal_true_body_is_fake(): Bool ->
    true.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0401:tests.fake]"));
    assert!(rendered.contains("literal true test body does not prove behavior"));
}

/// Verifies unannotated helpers in test files are not treated as tests.
#[test]
fn lint_accepts_unannotated_helper_identity_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

helper(value: Int): Bool ->
    assert_equal(value, value).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0401"),
        "unannotated helpers must not receive fake-test diagnostics: {diagnostics:?}"
    );
}

/// Verifies non-literal assertion conditions are accepted.
#[test]
fn lint_accepts_non_literal_assertion_condition_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub meaningful_assertion(): Bool ->
    assert(users.is_empty()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0401"),
        "non-literal assertion conditions must not be reported: {diagnostics:?}"
    );
}

/// Verifies fully qualified literal true assertions are covered in test files.
#[test]
fn lint_reports_qualified_literal_true_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub qualified_literal_true_assertion_is_fake(): Bool ->
    std.test.Test.assert(true).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0401"),
        "qualified literal true assertion should be reported: {diagnostics:?}"
    );
}

/// Verifies identity assertions in test files emit the fake-test rule.
#[test]
fn lint_reports_identity_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub identity_assertion_is_fake(): Bool ->
    assert_equal(user.name, user.name).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0401:tests.fake]"));
    assert!(rendered.contains("identity assertion does not prove behavior"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies non-identical assertions are not reported as fake.
#[test]
fn lint_accepts_non_identity_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub meaningful_assertion(): Bool ->
    assert_equal(expected_name, user.name).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0401"),
        "non-identical assertions must not be reported: {diagnostics:?}"
    );
}

/// Verifies declaration-only surface test names emit the fake-test rule.
#[test]
fn lint_reports_declaration_only_surface_test_name() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub surface_is_declared(): Bool ->
    assert(users.is_empty()).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0401:tests.fake]"));
    assert!(rendered.contains("declaration-only test name does not prove behavior"));
}

/// Verifies generated declaration-only surface test names are covered.
#[test]
fn lint_reports_generated_declaration_only_surface_test_name() {
    let diagnostics = lint_source(
        Path::new("GeneratedTest.terl"),
        r#"
module generated.GeneratedTest.

@test
pub generated_surface_is_declared(): Bool ->
    assert(generated_surface.is_present()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0401"),
        "generated declaration-only surface tests should be reported: {diagnostics:?}"
    );
}

/// Verifies declaration-only names are accepted outside `@test` declarations.
#[test]
fn lint_accepts_unannotated_declaration_only_helper_name_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

pub surface_is_declared(): Bool ->
    assert(users.is_empty()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0401"),
        "unannotated declaration-only helpers must not be reported: {diagnostics:?}"
    );
}

/// Verifies tests with too many assertions suggest table-driven coverage.
#[test]
fn lint_reports_oversized_test_assertion_volume() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub validates_many_rows_inline(): Bool ->
    assert_equal(1, first);
    assert_equal(2, second);
    assert_equal(3, third);
    assert_equal(4, fourth).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("suggestion[TL0402:tests.assertion-volume]"));
    assert!(rendered.contains("split it or use a table-driven test"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies focused tests at the assertion threshold are accepted.
#[test]
fn lint_accepts_focused_test_assertion_volume_threshold() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub validates_three_observations(): Bool ->
    assert_equal(1, first);
    assert_equal(2, second);
    assert_equal(3, third).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0402"),
        "three assertions should remain within the focused-test threshold: {diagnostics:?}"
    );
}

/// Verifies unannotated helpers are not checked for assertion volume.
#[test]
fn lint_accepts_unannotated_helper_assertion_volume_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

pub helper_with_many_assertions(): Bool ->
    assert_equal(1, first);
    assert_equal(2, second);
    assert_equal(3, third);
    assert_equal(4, fourth).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0402"),
        "unannotated helpers must not receive assertion-volume diagnostics: {diagnostics:?}"
    );
}

/// Verifies repeated equality rows suggest table-driven tests.
#[test]
fn lint_reports_repeated_assert_equal_table_candidate() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub validates_repeated_rows_inline(): Bool ->
    assert_equal(1, first);
    assert_equal(2, second);
    assert_equal(3, third).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("suggestion[TL0403:tests.table-driven-candidate]"));
    assert!(rendered.contains("repeated assert_equal rows should move to a table-driven test"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies two equality rows stay below the table suggestion threshold.
#[test]
fn lint_accepts_two_assert_equal_rows_without_table_candidate() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub validates_two_rows_inline(): Bool ->
    assert_equal(1, first);
    assert_equal(2, second).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0403"),
        "two equality rows should not require table-driven coverage: {diagnostics:?}"
    );
}

/// Verifies mixed assertion styles do not trigger table-driven suggestions.
#[test]
fn lint_accepts_mixed_assertions_without_table_candidate() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub validates_mixed_assertions(): Bool ->
    assert_equal(1, first);
    assert(first.is_valid());
    assert(second.is_valid()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0403"),
        "mixed assertion styles should not be treated as repeated rows: {diagnostics:?}"
    );
}

/// Verifies fake-test identity checks stay scoped to test files.
#[test]
fn lint_rejects_identity_assertion_rule_outside_test_source() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub compare(value: Int): Bool ->
    assert_equal(value, value).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0401"),
        "non-test source must not receive test-only identity diagnostics: {diagnostics:?}"
    );
}

/// Verifies fully-qualified std assertions are also covered in test files.
#[test]
fn lint_reports_qualified_identity_assertion_in_test_source() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

@test
pub qualified_identity_assertion_is_fake(): Bool ->
    std.test.Test.assert_equal(1 + count, 1 + count).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0401"),
        "qualified identity assertion should be reported: {diagnostics:?}"
    );
}
