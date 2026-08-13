use std::path::Path;

use super::lint_source;

#[cfg(test)]
#[path = "imports_test/redundant_test.rs"]
#[cfg(test)]
mod redundant_test;
#[cfg(test)]
#[path = "imports_test/selected_default_test.rs"]
#[cfg(test)]
mod selected_default_test;
#[cfg(test)]
#[path = "imports_test/unused_test.rs"]
#[cfg(test)]
mod unused_test;

/// Verifies exact duplicate imports emit a stable import diagnostic.
#[test]
fn lint_reports_duplicate_import_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.
import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:5:1"));
    assert!(rendered.contains("warning[TL0601:imports.duplicate]"));
    assert!(rendered.contains("duplicate import declaration"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies distinct selected imports are not treated as duplicates.
#[test]
fn lint_accepts_distinct_selected_imports() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.
import std.io.Console.{print}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0601"),
        "distinct selected imports must not be reported: {diagnostics:?}"
    );
}

/// Verifies comments that mention import syntax are ignored.
#[test]
fn lint_accepts_import_text_inside_comments() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

// import std.io.Console.{println}.
import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0601"),
        "comment text must not be treated as an import: {diagnostics:?}"
    );
}

/// Verifies duplicate selected import names emit a stable import diagnostic.
#[test]
fn lint_reports_duplicate_selected_import_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println, println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:1"));
    assert!(rendered.contains("warning[TL0602:imports.duplicate-selected]"));
    assert!(rendered.contains("duplicate selected import name"));
}

/// Verifies distinct selected import names stay accepted.
#[test]
fn lint_accepts_distinct_selected_import_names() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print, println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0602"),
        "distinct selected import names must not be reported: {diagnostics:?}"
    );
}

/// Verifies duplicate selected names are also covered for type imports.
#[test]
fn lint_reports_duplicate_selected_type_import_name() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import type std.collections.{List, List}.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0602"),
        "duplicate selected type import names should be reported: {diagnostics:?}"
    );
}

/// Verifies import declarations should be sorted.
#[test]
fn lint_reports_unsorted_import_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.
import std.collections.List.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:5:1"));
    assert!(rendered.contains("warning[TL0603:imports.sort-order]"));
    assert!(rendered.contains("import declarations should be sorted"));
}

/// Verifies sorted imports are accepted.
#[test]
fn lint_accepts_sorted_import_declarations() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.collections.List.
import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0603"),
        "sorted import declarations must be accepted: {diagnostics:?}"
    );
}

/// Verifies block-comment import examples are ignored by import rules.
#[test]
fn lint_accepts_import_text_inside_block_comments() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * import std.z.Bad.
 * import std.a.Bad.
 */
import std.collections.List.
import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0603"),
        "block comment text must not be treated as imports: {diagnostics:?}"
    );
}

/// Verifies selected import names should be sorted.
#[test]
fn lint_reports_unsorted_selected_import_names() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println, print}.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:4:1"));
    assert!(rendered.contains("warning[TL0604:imports.selected-sort-order]"));
    assert!(rendered.contains("selected import names should be sorted"));
}

/// Verifies sorted selected import names are accepted.
#[test]
fn lint_accepts_sorted_selected_import_names() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print, println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0604"),
        "sorted selected import names must be accepted: {diagnostics:?}"
    );
}

/// Verifies sorted type selected import names are also accepted.
#[test]
fn lint_accepts_sorted_type_selected_import_names() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import type std.collections.{List, Map}.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0604"),
        "sorted selected type import names must be accepted: {diagnostics:?}"
    );
}

/// Verifies repeated selected imports from one module should be grouped.
#[test]
fn lint_reports_split_selected_imports_from_same_module() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print}.
import std.io.Console.{println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:5:1"));
    assert!(rendered.contains("warning[TL0605:imports.grouped-selected]"));
    assert!(rendered.contains("selected imports from the same module should be grouped"));
}

/// Verifies grouped selected imports from one module are accepted.
#[test]
fn lint_accepts_grouped_selected_imports_from_same_module() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{print, println}.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0605"),
        "grouped selected imports must be accepted: {diagnostics:?}"
    );
}

/// Verifies value and type selected imports remain separate groups.
#[test]
fn lint_accepts_separate_value_and_type_selected_imports() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.collections.Iterator.{next}.
import type std.collections.Iterator.{Iterator}.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0605"),
        "value and type selected imports are distinct groups: {diagnostics:?}"
    );
}
