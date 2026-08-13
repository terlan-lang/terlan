use std::path::Path;

use crate::commands::lint::render_diagnostic;

use super::lint_source;

#[cfg(test)]
#[path = "readability_test/branch_test.rs"]
mod branch_test;

/// Verifies deeply nested expression trees receive a readability warning.
#[test]
fn lint_reports_deep_expression_tree() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(input: Int): Int ->
    wrap(wrap(wrap(wrap(wrap(wrap(wrap(wrap(wrap(input))))))))).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0002:readability.deep-expression]"));
    assert!(rendered.contains("deep expression tree should be split"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies staged bindings avoid the deep-expression warning.
#[test]
fn lint_accepts_staged_expression_tree() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(input: Int): Int ->
    let first = wrap(input);
        second = wrap(first);
        third = wrap(second);
        fourth = wrap(third);
    wrap(fourth).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0002"),
        "staged expression should not receive deep-expression diagnostics: {diagnostics:?}"
    );
}

#[test]
fn lint_reports_two_linear_cases_with_one_repeated_fallback() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

resolve(first: Option[Int], second: Option[Int]): Option[Int] ->
    case first {
        None -> None;
        Some(left) -> case second {
            None -> None;
            Some(right) -> Some(left + right)
        }
    }.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0009:readability.grouped-binding]"));
    assert!(rendered.contains("should use grouped `let { ... } else { ... }` bindings"));
    assert!(!rendered.contains("[fix available]"));
}

#[test]
fn lint_accepts_nested_cases_with_distinct_failure_behavior() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

resolve(first: Option[Int], second: Option[Int]): Result[Int, String] ->
    case first {
        None -> Err("first");
        Some(left) -> case second {
            None -> Err("second");
            Some(right) -> Ok(left + right)
        }
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0009"),
        "distinct fallback behavior must retain explicit cases: {diagnostics:?}"
    );
}

#[test]
fn lint_accepts_repeated_fallback_that_reads_failure_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

resolve(first: Result[Int, String], second: Result[Int, String]): Result[Int, String] ->
    case first {
        Err(reason) -> Err(reason);
        Ok(left) -> case second {
            Err(reason) -> Err(reason);
            Ok(right) -> Ok(left + right)
        }
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0009"),
        "failure-binding-dependent cases cannot use a wildcard grouped else: {diagnostics:?}"
    );
}

#[test]
fn lint_accepts_grouped_refutable_bindings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

resolve(first: Option[Int], second: Option[Int]): Option[Int] ->
    let {
        Some(left) <- first;
        Some(right) <- second
    } else {
        None -> None
    };
    Some(left + right).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0009"),
        "canonical grouped bindings must be accepted: {diagnostics:?}"
    );
}

#[test]
fn lint_reports_lambda_that_only_forwards_its_parameter() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

property(generator: Gen[Int]): Bool ->
    for_all(generator, (width) -> generated(width)).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0010:readability.function-reference]"));
    assert!(rendered.contains("should be a direct function reference"));
}

#[test]
fn lint_accepts_direct_function_reference() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

property(generator: Gen[Int]): Bool ->
    for_all(generator, generated).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0010"),
        "direct function references are canonical: {diagnostics:?}"
    );
}

#[test]
fn lint_accepts_lambda_that_transforms_its_parameter() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

property(generator: Gen[Int]): Bool ->
    for_all(generator, (width) -> generated(width + 1)).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0010"),
        "argument-transforming lambdas must remain explicit: {diagnostics:?}"
    );
}

/// Verifies property tests may keep inline setup callbacks required by the VM runner.
#[test]
fn lint_accepts_property_test_inline_setup_callbacks() {
    let diagnostics = lint_source(
        Path::new("MapPropertyTest.terl"),
        r#"
module sample.MapPropertyTest.

pub generated_key_put_updates_presence_and_size(): Bool ->
    for_all(
        elements(["alpha", "beta", "gamma"]),
        (key) ->
            let state = Map({key, 1});
            state.remove(key);
            assert_false(state.contains_key(key)) and state.size() == 0
    ).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0002"),
        "property-test setup callback should not receive deep-expression diagnostics: {diagnostics:?}"
    );
}

/// Verifies multi-expression callbacks reject throwaway parameter names.
#[test]
fn lint_reports_short_callback_name_for_multi_expression_body() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub render(values: List[Int]): Unit ->
    values.each((x) ->
        let doubled = x + x;
        Console.println(doubled)
    ).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0003:readability.callback-name]"));
    assert!(rendered.contains("multi-expression callbacks should use meaningful parameter names"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies multi-expression callbacks accept names that explain the value.
#[test]
fn lint_accepts_meaningful_callback_name_for_multi_expression_body() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub render(values: List[Int]): Unit ->
    values.each((value) ->
        let doubled = value + value;
        Console.println(doubled)
    ).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0003"),
        "meaningful callback name should not receive callback-name diagnostics: {diagnostics:?}"
    );
}

/// Verifies compact one-expression callbacks can still use short names.
#[test]
fn lint_accepts_short_callback_name_for_single_expression_body() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub double(values: List[Int]): List[Int] ->
    values.map((x) -> x + x).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0003"),
        "single-expression callback should not receive callback-name diagnostics: {diagnostics:?}"
    );
}

/// Verifies destructured bindings must be used or intentionally ignored.
#[test]
fn lint_reports_unused_destructured_let_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub label(pair: {Int, String}): String ->
    let {id, label} = pair;
    label.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0004:readability.unused-destructure-binding]"));
    assert!(rendered.contains("unused destructured bindings should use `_`"));
    assert_eq!(
        1,
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule_id == "TL0004")
            .count()
    );
}

/// Verifies intentionally ignored destructured names stay accepted.
#[test]
fn lint_accepts_underscore_prefixed_destructured_let_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub label(pair: {Int, String}): String ->
    let {_id, label} = pair;
    label.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0004"),
        "underscore-prefixed destructured bindings should be accepted: {diagnostics:?}"
    );
}

/// Verifies used destructured names do not receive unused-binding warnings.
#[test]
fn lint_accepts_used_destructured_let_bindings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub render(pair: {Int, String}): String ->
    let {id, label} = pair;
    id.to_string() + label.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0004"),
        "used destructured bindings should be accepted: {diagnostics:?}"
    );
}

/// Verifies clause patterns are covered by the same destructuring rule.
#[test]
fn lint_reports_unused_destructured_case_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub label(pair: {Int, String}): String ->
    case pair {
        {id, label} -> label
    }.
"#,
    );

    assert_eq!(
        1,
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule_id == "TL0004")
            .count(),
        "unused destructured case bindings should be reported once: {diagnostics:?}"
    );
}

/// Verifies comments that repeat the next expression are reported.
#[test]
fn lint_reports_redundant_comment_restatement() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub clear(values: List[Int]): Unit ->
    // values.clear()
    values.clear().
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0005:readability.redundant-comment]"));
    assert!(rendered.contains("comments should explain intent"));
}

/// Verifies intent comments are not treated as restatements.
#[test]
fn lint_accepts_explanatory_line_comment() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub clear(values: List[Int]): Unit ->
    // Clear stale cache entries before reuse.
    values.clear().
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0005"),
        "explanatory comments should not receive restatement diagnostics: {diagnostics:?}"
    );
}

/// Verifies doc comments are outside the redundant-comment rule.
#[test]
fn lint_accepts_doc_comment_before_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/// clear.
pub clear(values: List[Int]): Unit ->
    values.clear().
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0005"),
        "doc comments should not receive restatement diagnostics: {diagnostics:?}"
    );
}

/// Verifies undocumented public declarations are reported.
#[test]
fn lint_reports_public_declaration_without_docs() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Int ->
    1.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0006:readability.public-docs]"));
    assert!(rendered.contains("public API declarations should have documentation"));
}

/// Verifies documented public declarations satisfy the docs rule.
#[test]
fn lint_accepts_documented_public_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * Returns a stable sample value.
 */
pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0006"),
        "documented public declarations should be accepted: {diagnostics:?}"
    );
}

/// Verifies public declarations in test files are exempt from docs linting.
#[test]
fn lint_accepts_undocumented_public_test_declaration() {
    let diagnostics = lint_source(
        Path::new("SampleTest.terl"),
        r#"
module sample.SampleTest.

pub value_is_stable(): Bool ->
    true.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0006"),
        "test-only source should be exempt from public docs linting: {diagnostics:?}"
    );
}

/// Verifies malformed block-doc star spacing is reported.
#[test]
fn lint_reports_doc_comment_missing_star_space() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 *Good docs need spacing.
 */
pub value(): Int ->
    1.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0007:readability.doc-comment-spacing]"));
    assert!(rendered.contains("block doc lines should use ` * ` spacing"));
}

/// Verifies canonical block-doc spacing is accepted.
#[test]
fn lint_accepts_canonical_doc_comment_spacing() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * Good docs use spacing.
 */
pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0007"),
        "canonical block-doc spacing should be accepted: {diagnostics:?}"
    );
}

/// Verifies ordinary block comments are outside the doc-spacing rule.
#[test]
fn lint_ignores_non_doc_block_comment_spacing() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/*
 *Internal note can stay non-doc.
 */

/**
 * Good docs use spacing.
 */
pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0007"),
        "non-doc block comments should not receive doc-spacing diagnostics: {diagnostics:?}"
    );
}
