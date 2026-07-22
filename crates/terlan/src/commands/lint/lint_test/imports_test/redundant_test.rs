use std::path::Path;

use super::super::lint_source;

/// Verifies selected imports make module-qualified calls redundant.
#[test]
fn lint_reports_redundant_module_qualified_selected_import_call() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.

pub main(): Unit ->
    Console.println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:7:5"));
    assert!(rendered.contains("suggestion[TL0608:imports.redundant-qualifier]"));
    assert!(rendered.contains("selected import already makes this call unambiguous"));
}

/// Verifies selected imports also make full-qualified calls redundant.
#[test]
fn lint_reports_redundant_full_qualified_selected_import_call() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.

pub main(): Unit ->
    std.io.Console.println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0608"),
        "fully qualified selected import call should be reported: {diagnostics:?}"
    );
}

/// Verifies direct selected-name calls remain accepted.
#[test]
fn lint_accepts_direct_selected_import_call() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0608"),
        "direct selected import calls must remain accepted: {diagnostics:?}"
    );
}

/// Verifies alias-selected imports still reject redundant source-name calls.
#[test]
fn lint_reports_redundant_qualified_call_for_aliased_selected_import() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println as write_line}.

pub main(): Unit ->
    Console.println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0608"),
        "qualified call should be redundant even when selected import has alias: {diagnostics:?}"
    );
}

/// Verifies type and constructor selected imports remain outside this rule.
#[test]
fn lint_accepts_type_and_constructor_selected_imports_for_redundant_qualifier_rule() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.core.Option.{None, Some}.
import type std.collections.{List}.

pub main(items: List[Int]): Unit ->
    Option.Some(items.length()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0608"),
        "type and constructor selected imports must not trigger redundant qualifier lint: {diagnostics:?}"
    );
}

/// Verifies comments and strings are ignored by the qualifier scanner.
#[test]
fn lint_accepts_qualified_selected_import_text_inside_comments_and_strings() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.

pub main(): String ->
    // Console.println("hello")
    "std.io.Console.println(\"hello\")".
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0608"),
        "comments and strings must not trigger redundant qualifier lint: {diagnostics:?}"
    );
}
