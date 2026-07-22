use std::path::Path;

use super::lint_source;

/// Verifies camelCase function names are reported.
#[test]
fn lint_reports_camel_case_function_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub binarySearch(): Int ->
    1.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:5"));
    assert!(rendered.contains("warning[TL0301:naming.function-snake-case]"));
    assert!(rendered.contains("function and method names should use lower_snake_case"));
}

/// Verifies lower_snake_case function names are accepted.
#[test]
fn lint_accepts_snake_case_function_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub binary_search(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0301"),
        "lower_snake_case functions must be accepted: {diagnostics:?}"
    );
}

/// Verifies receiver method names are covered by the same rule.
#[test]
fn lint_reports_camel_case_method_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

struct User {
    name: String
}.

pub (self: User) displayName(): String ->
    self.name.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0301"),
        "camelCase receiver methods should be reported: {diagnostics:?}"
    );
}

/// Verifies UpperCamelCase type names are not function-name violations.
#[test]
fn lint_accepts_upper_camel_type_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub type UserProfile =
    String.

pub user_profile(): String ->
    "ok".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0301"),
        "UpperCamelCase type names must not be checked by the function rule: {diagnostics:?}"
    );
}

/// Verifies noncanonical type names accepted by the parser are reported.
#[test]
fn lint_reports_noncanonical_type_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub type User_Profile =
    String.

pub user_profile(): String ->
    "ok".
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0302:naming.type-upper-camel]"));
    assert!(
        rendered.contains("type, struct, trait, and constructor names should use UpperCamelCase")
    );
}

/// Verifies noncanonical struct names accepted by the parser are reported.
#[test]
fn lint_reports_noncanonical_struct_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub struct User_Profile {
    name: String
}.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0302"),
        "noncanonical struct names should be reported: {diagnostics:?}"
    );
}

/// Verifies UpperCamelCase struct names are accepted.
#[test]
fn lint_accepts_upper_camel_struct_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub struct UserProfile {
    name: String
}.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0302"),
        "UpperCamelCase struct names must be accepted: {diagnostics:?}"
    );
}

/// Verifies function names cannot differ only by case and underscores.
#[test]
fn lint_reports_case_underscore_function_name_collision() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub user_id(): Int ->
    1.

pub userId(): Int ->
    2.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0303:naming.case-underscore-collision]"));
    assert!(rendered.contains("declaration names should not differ only by case or underscores"));
}

/// Verifies type names cannot differ only by case and underscores.
#[test]
fn lint_reports_case_underscore_type_name_collision() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub type UserId =
    Int.

pub type User_ID =
    Int.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0303"),
        "type names that differ only by case and underscores should be reported: {diagnostics:?}"
    );
}

/// Verifies meaningfully distinct declaration names are accepted.
#[test]
fn lint_accepts_distinct_declaration_names() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub user_id(): Int ->
    1.

pub account_id(): Int ->
    2.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0303"),
        "distinct declaration names must be accepted: {diagnostics:?}"
    );
}

/// Verifies camelCase function parameter bindings are reported.
#[test]
fn lint_reports_camel_case_function_parameter_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub greet(userName: String): String ->
    userName.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("warning[TL0304:naming.binding-snake-case]"));
    assert!(rendered.contains("value bindings should use lower_snake_case"));
}

/// Verifies expression-local pattern bindings are reported.
#[test]
fn lint_reports_camel_case_let_pattern_binding() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(input: {Int, Int}): Int ->
    let {userId, _} = input;
    userId.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0304"),
        "camelCase let pattern bindings should be reported: {diagnostics:?}"
    );
}

/// Verifies expression-local snake-case bindings are accepted.
#[test]
fn lint_accepts_snake_case_value_bindings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(user_name: String, input: {Int, Int}): String ->
    let {user_id, _} = input;
    user_name.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0304"),
        "snake_case value bindings must be accepted: {diagnostics:?}"
    );
}

/// Verifies intentionally unused bindings may start with an underscore.
#[test]
fn lint_accepts_underscore_prefixed_value_bindings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(input: {Int, Int}): Int ->
    let {_unused_value, kept_value} = input;
    kept_value.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0304"),
        "underscore-prefixed unused bindings must be accepted: {diagnostics:?}"
    );
}
