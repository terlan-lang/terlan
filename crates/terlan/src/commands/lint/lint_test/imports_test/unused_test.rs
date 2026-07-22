use std::path::Path;

use super::super::lint_source;

/// Verifies unused selected value imports emit a stable diagnostic.
#[test]
fn lint_reports_unused_selected_value_import() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print, println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:1"));
    assert!(rendered.contains("warning[TL0606:imports.unused-selected]"));
    assert!(rendered.contains("selected import name is unused"));
}

/// Verifies selected imports used by value name stay accepted.
#[test]
fn lint_accepts_used_selected_value_imports() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print, println}.

pub main(): Unit ->
    print("hello");
    println("world").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0606"),
        "used selected value imports must be accepted: {diagnostics:?}"
    );
}

/// Verifies selected type imports count type annotations as use sites.
#[test]
fn lint_accepts_used_selected_type_import() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import type std.collections.{List}.

pub value(items: List[Int]): Int ->
    items.length().
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0606"),
        "used selected type imports must be accepted: {diagnostics:?}"
    );
}

/// Verifies constructor-shaped imports are not reported by the textual lint.
#[test]
fn lint_accepts_constructor_shaped_selected_import_without_textual_use() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.core.Option.{None, Option, Some}.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0606"),
        "constructor-shaped selected imports are resolved by compiler: {diagnostics:?}"
    );
}

/// Verifies selected import aliases are checked through their visible name.
#[test]
fn lint_accepts_used_selected_import_alias() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println as write_line}.

pub main(): Unit ->
    write_line("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0606"),
        "used selected import alias must be accepted: {diagnostics:?}"
    );
}

/// Verifies ordinary unused module imports emit a stable diagnostic.
#[test]
fn lint_reports_unused_module_import() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): Unit ->
    1.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:1"));
    assert!(rendered.contains("warning[TL0607:imports.unused-module]"));
    assert!(rendered.contains("module import is unused"));
}

/// Verifies ordinary module imports used by visible module name stay accepted.
#[test]
fn lint_accepts_used_module_import_by_visible_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.

pub main(): Unit ->
    Console.println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0607"),
        "visible module use must satisfy ordinary imports: {diagnostics:?}"
    );
}

/// Verifies ordinary module imports used by full module path stay accepted.
#[test]
fn lint_accepts_used_module_import_by_qualified_path() {
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
            .all(|diagnostic| diagnostic.rule_id != "TL0607"),
        "fully qualified module use must satisfy ordinary imports: {diagnostics:?}"
    );
}

/// Verifies ordinary module import aliases are checked through their alias.
#[test]
fn lint_accepts_used_module_import_alias() {
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
            .all(|diagnostic| diagnostic.rule_id != "TL0607"),
        "module import alias use must satisfy ordinary imports: {diagnostics:?}"
    );
}

/// Verifies selected imports are owned by the selected-import lint rule.
#[test]
fn lint_accepts_selected_imports_for_module_unused_rule() {
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
            .all(|diagnostic| diagnostic.rule_id != "TL0607"),
        "selected imports must not be reported by module rule: {diagnostics:?}"
    );
}

/// Verifies type and asset imports remain outside the module-unused rule.
#[test]
fn lint_accepts_type_and_asset_imports_for_module_unused_rule() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import css "./app.css" as AppCss.
import file "./logo.txt" as Logo.
import type std.collections.List.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0607"),
        "type and asset imports must not be reported by module rule: {diagnostics:?}"
    );
}
