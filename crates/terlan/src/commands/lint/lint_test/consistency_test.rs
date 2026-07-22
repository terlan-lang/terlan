use std::path::Path;

use super::lint_source;

/// Verifies imports before the module declaration are rejected.
#[test]
fn lint_reports_import_before_module_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
import std.io.Console.{println}.

module sample.

pub main(): Unit ->
    println("hello").
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:2:1"));
    assert!(rendered.contains("error[TL0501:consistency.module-order]"));
    assert!(rendered.contains("module declaration must be the first non-comment declaration"));
}

/// Verifies leading module docs are allowed before the module declaration.
#[test]
fn lint_accepts_docs_before_module_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
/**
 * Sample module.
 */
module sample.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0501"),
        "leading docs before the module declaration must be accepted: {diagnostics:?}"
    );
}

/// Verifies leading line comments are allowed before the module declaration.
#[test]
fn lint_accepts_line_comments_before_module_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
// generated for test fixture
module sample.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0501"),
        "leading line comments before the module declaration must be accepted: {diagnostics:?}"
    );
}

/// Verifies imports after declarations are rejected.
#[test]
fn lint_reports_import_after_function_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Int ->
    1.

import std.io.Console.{println}.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:7:1"));
    assert!(rendered.contains("error[TL0502:consistency.import-order]"));
    assert!(rendered.contains("imports must appear before"));
}

/// Verifies normal import blocks before declarations are accepted.
#[test]
fn lint_accepts_imports_before_declarations() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.
import type std.collections.List.

pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0502"),
        "imports before declarations must be accepted: {diagnostics:?}"
    );
}

/// Verifies declaration docs do not make later declarations look like imports.
#[test]
fn lint_accepts_docs_between_imports_and_declarations() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

import std.io.Console.{println}.

/**
 * Main entrypoint.
 */
pub main(): Unit ->
    println("hello").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0502"),
        "docs between imports and declarations must be accepted: {diagnostics:?}"
    );
}

/// Verifies duplicate module declarations are rejected.
#[test]
fn lint_reports_duplicate_module_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Int ->
    1.

module other.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:7:1"));
    assert!(rendered.contains("error[TL0503:consistency.single-module]"));
    assert!(rendered.contains("a source file may declare exactly one module"));
}

/// Verifies module words inside comments and docs are not duplicate modules.
#[test]
fn lint_accepts_module_mentions_inside_comments() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
/**
 * This module is generated.
 */
module sample.

// The module keeps one public function.
pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0503"),
        "module mentions inside comments must not count as duplicate declarations: {diagnostics:?}"
    );
}

/// Verifies type-like declarations after functions are rejected.
#[test]
fn lint_reports_type_declaration_after_function_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Int ->
    1.

pub type Count = Int.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:7:1"));
    assert!(rendered.contains("warning[TL0504:consistency.declaration-order]"));
    assert!(rendered.contains("declarations should be ordered as types"));
}

/// Verifies impl declarations after functions are rejected.
#[test]
fn lint_reports_impl_declaration_after_function_declaration() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Int ->
    1.

impl Show[Count].
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Sample.terl:7:1"));
    assert!(rendered.contains("warning[TL0504:consistency.declaration-order]"));
}

/// Verifies the canonical declaration block order is accepted.
#[test]
fn lint_accepts_type_impl_function_declaration_order() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub type Count = Int.

impl Show[Count].

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0504"),
        "canonical declaration order must be accepted: {diagnostics:?}"
    );
}

/// Verifies std source modules must match their canonical source path.
#[test]
fn lint_reports_std_module_declaration_path_mismatch() {
    let diagnostics = lint_source(
        Path::new("std/core/Bool.terl"),
        r#"
module std.core.Boolean.

pub value(): Bool ->
    true.
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("std/core/Bool.terl:2:1"));
    assert!(rendered.contains("error[TL0505:consistency.std-module-path]"));
    assert!(rendered.contains("std module declaration must match"));
}

/// Verifies std source modules matching their path remain accepted.
#[test]
fn lint_accepts_std_module_declaration_matching_path() {
    let diagnostics = lint_source(
        Path::new("std/core/BoolTest.terl"),
        r#"
module std.core.BoolTest.

pub value(): Bool ->
    true.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0505"),
        "matching std module declarations must be accepted: {diagnostics:?}"
    );
}

/// Verifies non-std fixtures are not forced into std path naming.
#[test]
fn lint_accepts_non_std_module_declaration_path_mismatch() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub value(): Bool ->
    true.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0505"),
        "non-std source paths must stay outside std path linting: {diagnostics:?}"
    );
}

/// Verifies module mentions inside comments do not affect std path matching.
#[test]
fn lint_accepts_std_module_path_with_comment_module_mentions() {
    let diagnostics = lint_source(
        Path::new("std/core/Bool.terl"),
        r#"
/**
 * module std.core.Boolean.
 */
module std.core.Bool.

pub value(): Bool ->
    true.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0505"),
        "commented module mentions must not affect std path linting: {diagnostics:?}"
    );
}

/// Verifies comments and docs between declaration blocks do not affect order.
#[test]
fn lint_accepts_comments_between_declaration_blocks() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

/**
 * Count alias.
 */
pub type Count = Int.

// Implementation docs can sit between blocks.
impl Show[Count].

/**
 * Returns one.
 */
pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0504"),
        "comments between declaration blocks must be accepted: {diagnostics:?}"
    );
}

/// Verifies methods inside impl blocks do not count as top-level functions.
#[test]
fn lint_accepts_multiple_impl_blocks_with_methods_before_functions() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub trait Show[T] {
    show(value: T): String.
}.

pub type Count = Int.

impl Show[Count].
    show(value: Count): String ->
        "count".

impl Eq[Count].
    equals(left: Count, right: Count): Bool ->
        left == right.

pub value(): Int ->
    1.
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0504"),
        "impl methods must not be treated as top-level functions: {diagnostics:?}"
    );
}
