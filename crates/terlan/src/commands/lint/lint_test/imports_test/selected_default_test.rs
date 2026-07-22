use std::path::Path;

use super::super::lint_source;

/// Verifies module imports used for one member suggest selected imports.
#[test]
fn lint_reports_default_import_that_could_be_selected() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): Unit ->
    Console.println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:1"));
    assert!(rendered.contains("suggestion[TL0609:imports.default-could-be-selected]"));
    assert!(rendered.contains("module import is used for one member"));
}

/// Verifies full qualified one-member use also suggests a selected import.
#[test]
fn lint_reports_full_qualified_default_import_that_could_be_selected() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): Unit ->
    std.io.Console.println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0609"),
        "single full-qualified member use should be reported: {diagnostics:?}"
    );
}

/// Verifies multi-member module use remains a useful whole-module import.
#[test]
fn lint_accepts_default_import_used_for_multiple_members() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): Unit ->
    Console.print("hello");
    Console.println("world").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0609"),
        "multi-member module use must stay accepted: {diagnostics:?}"
    );
}

/// Verifies aliased module imports remain outside selected-import suggestions.
#[test]
fn lint_accepts_aliased_default_import() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console as C.

pub main(): Unit ->
    C.println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0609"),
        "aliased module imports must stay accepted: {diagnostics:?}"
    );
}

/// Verifies constructor-looking member calls are not treated as selected values.
#[test]
fn lint_accepts_constructor_shaped_member_use() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.core.Option.

pub main(): Unit ->
    Option.Some(1).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0609"),
        "constructor-shaped member calls must stay accepted: {diagnostics:?}"
    );
}

/// Verifies module-visible constructor calls keep default imports valid.
#[test]
fn lint_accepts_default_import_used_for_constructor_and_member() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.collections.List.

pub main(): Int ->
    let values = List(1, 2);
    values.length() + List.new().length().
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0609"),
        "constructor shorthand must keep default imports accepted: {diagnostics:?}"
    );
}

/// Verifies comments and strings do not create selected-import suggestions.
#[test]
fn lint_accepts_default_import_member_text_inside_comments_and_strings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): String ->
    // Console.println("hello")
    "std.io.Console.println(\"hello\")".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0609"),
        "comments and strings must not trigger selected import suggestions: {diagnostics:?}"
    );
}
