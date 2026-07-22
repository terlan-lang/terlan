use std::path::Path;

use crate::commands::lint::render_diagnostic;

use super::super::lint_source;

/// Verifies branch conditions with multiple boolean operators are reported.
#[test]
fn lint_reports_boolean_heavy_branch_condition() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub allowed(user: User): Bool ->
    if {
        user.active and user.verified or user.admin -> true;
        _ -> false
    }.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0008:readability.boolean-heavy-branch]"));
    assert!(rendered.contains("boolean-heavy branch condition"));
}

/// Verifies compact branch conditions with one connective remain accepted.
#[test]
fn lint_accepts_simple_boolean_branch_condition() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub allowed(user: User): Bool ->
    if {
        user.active and user.verified -> true;
        _ -> false
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0008"),
        "one boolean connective should remain accepted: {diagnostics:?}"
    );
}

/// Verifies named predicates avoid branch-condition noise.
#[test]
fn lint_accepts_named_predicate_branch_condition() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub allowed(user: User): Bool ->
    if {
        can_access(user) -> true;
        _ -> false
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0008"),
        "named predicates should stay accepted: {diagnostics:?}"
    );
}

/// Verifies comments with boolean-heavy examples are ignored.
#[test]
fn lint_accepts_boolean_heavy_branch_text_inside_comments() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub allowed(user: User): Bool ->
    // user.active and user.verified or user.admin -> true
    if {
        can_access(user) -> true;
        _ -> false
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0008"),
        "comment examples should not trigger branch linting: {diagnostics:?}"
    );
}
