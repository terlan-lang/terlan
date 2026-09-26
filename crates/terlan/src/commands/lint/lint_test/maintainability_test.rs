use std::path::Path;

use super::lint_source;

/// Verifies unstructured debug calls in production source are reported.
#[test]
fn lint_reports_unstructured_debug_call_in_production_source() {
    let diagnostics = lint_source(
        Path::new("Debug.terl"),
        r#"
module sample.Debug.

/**
 * Runs the production path.
 */
pub run(user_id: String): Unit ->
    debug(user_id).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Debug.terl:8:5"));
    assert!(rendered.contains("warning[TL0904:maintainability.debug-call]"));
    assert!(rendered.contains("debug-style calls in production source"));
}

/// Verifies debug words in strings or comments are ignored.
#[test]
fn lint_accepts_debug_words_in_strings_and_comments() {
    let diagnostics = lint_source(
        Path::new("Strings.terl"),
        r#"
module sample.Strings.

/**
 * Runs the production path.
 */
pub run(): String ->
    // debug(user) is documentation, not executable code.
    "debug(user)".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0904"),
        "strings and comments must not trigger debug-call lint: {diagnostics:?}"
    );
}

/// Verifies structured logging style remains accepted.
#[test]
fn lint_accepts_structured_logger_debug_call() {
    let diagnostics = lint_source(
        Path::new("Structured.terl"),
        r#"
module sample.Structured.

/**
 * Runs the production path.
 */
pub run(message: String): Unit ->
    Logger.debug(message).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0904"),
        "structured logger calls must remain accepted: {diagnostics:?}"
    );
}

/// Verifies test files may use debug helpers while being developed.
#[test]
fn lint_accepts_debug_call_in_test_source() {
    let diagnostics = lint_source(
        Path::new("DebugTest.terl"),
        r#"
module sample.DebugTest.

@test
pub debug_helper_can_exist_in_test(): Bool ->
    debug("fixture");
    assert_equal(1, 1 + 0).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0904"),
        "test source must not trigger production debug-call lint: {diagnostics:?}"
    );
}

/// Verifies oversized embedded SQL clauses are rejected.
#[test]
fn lint_reports_embedded_sql_over_100_columns() {
    let diagnostics = lint_source(
        Path::new("Query.terl"),
        r#"
module sample.Query.

pub query(email: String): Result[Option[Row], Error] ->
    sql[Row] {
        SELECT id, username, email, password_hash, rating, created_at, last_login_at, locked_until_at FROM users
        WHERE email = ${email}
    }.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("error[TL0907:maintainability.sql-line-width]"));
}

/// Verifies clause-oriented multiline SQL stays within the lint boundary.
#[test]
fn lint_accepts_wrapped_embedded_sql() {
    let diagnostics = lint_source(
        Path::new("Query.terl"),
        r#"
module sample.Query.

pub query(email: String): Result[Option[Row], Error] ->
    sql[Row] {
        SELECT id, username, email
        FROM users
        WHERE email = ${email}
        LIMIT 1
    }.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0907"),
        "wrapped SQL must remain accepted: {diagnostics:?}"
    );
}
