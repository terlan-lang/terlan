use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::commands::lint::render_diagnostic;
use crate::support::test_fs::{temp_dir, write_file};

use super::{lint_source, run};

/// Verifies safe nested first-argument module calls produce pipe suggestions.
#[test]
fn lint_reports_safe_nested_module_call_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(Set.iterator(collection), cb).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("suggestion[TL1002:format-boundary.pipe-fix]"));
    assert!(rendered.contains("Sample.terl:5:5"));
    assert!(rendered.contains("prefer pipe form"));
    assert!(rendered.contains("[fix available]"));
}

/// Verifies pipe fixing rewrites only the safe nested module-call shape.
#[test]
fn lint_command_fix_rewrites_safe_nested_module_call_pipe_candidate() {
    let root = temp_dir("lint_command", "pipe_fix");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        r#"module app.Main.

/**
 * Consumes each collection item.
 */
pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(Set.iterator(collection), cb).
"#,
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(
        fixed.contains("    collection\n        |> Set.iterator()\n        |> Iterator.each(cb).")
    );
    assert!(!fixed.contains("Iterator.each(Set.iterator(collection), cb)."));
}

/// Verifies pipe diagnostics use each expression span, not the first matching
/// rendered text.
#[test]
fn lint_reports_repeated_pipe_candidates_at_distinct_lines() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(Set.iterator(collection), cb);
    Iterator.each(Set.iterator(collection), cb).
"#,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.rule_id == "TL1002")
    .collect::<Vec<_>>();

    assert_eq!(diagnostics.len(), 2, "expected two pipe diagnostics");
    assert_eq!(diagnostics[0].line, 5);
    assert_eq!(diagnostics[1].line, 6);
}

/// Verifies pipe fixing does not rewrite matching text inside string literals.
#[test]
fn lint_command_fix_uses_pipe_candidate_spans_not_matching_string_text() {
    let root = temp_dir("lint_command", "pipe_fix_string_lookalike");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        r#"module app.Main.

/**
 * Keeps example text while rewriting the real call.
 */
pub each(collection: Set, cb: Callback): Unit ->
    let example = "Iterator.each(Set.iterator(collection), cb)";
    Iterator.each(Set.iterator(collection), cb).
"#,
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(fixed.contains("\"Iterator.each(Set.iterator(collection), cb)\""));
    assert!(
        fixed.contains("    collection\n        |> Set.iterator()\n        |> Iterator.each(cb).")
    );
}

/// Verifies safe receiver calls used as first arguments produce pipe suggestions.
#[test]
fn lint_reports_safe_receiver_inner_call_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(collection.iterator(), cb).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("suggestion[TL1002:format-boundary.pipe-fix]"));
    assert!(rendered.contains("prefer pipe form"));
    assert!(rendered.contains("[fix available]"));
}

/// Verifies receiver-call pipe fixing preserves the receiver method stage.
#[test]
fn lint_command_fix_rewrites_safe_receiver_inner_call_pipe_candidate() {
    let root = temp_dir("lint_command", "pipe_fix_receiver");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        r#"module app.Main.

/**
 * Consumes each collection item.
 */
pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(collection.iterator(), cb).
"#,
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(fixed.contains("    collection\n        |> iterator()\n        |> Iterator.each(cb)."));
    assert!(!fixed.contains("Iterator.each(collection.iterator(), cb)."));
}

/// Verifies declared local helpers can participate in safe pipe chains.
#[test]
fn lint_command_fix_rewrites_declared_local_helper_pipe_stage() {
    let root = temp_dir("lint_command", "pipe_fix_local_helper");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        r#"module app.Main.

/**
 * Converts a collection through a local helper.
 */
pub collect(collection: Set, cb: Callback): List ->
    Consumer.consume(map_iterator(Set.iterator(collection), cb), "done").

map_iterator(iterator: Iterator, cb: Callback): Iterator ->
    Iterator.map(iterator, cb).
"#,
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(fixed.contains(
        "    collection\n        |> Set.iterator()\n        |> map_iterator(cb)\n        |> Consumer.consume(\"done\")."
    ));
    assert!(
        !fixed.contains("Consumer.consume(map_iterator(Set.iterator(collection), cb), \"done\").")
    );
}

/// Verifies selected imports can participate in safe pipe chains.
#[test]
fn lint_command_fix_rewrites_selected_import_pipe_stage() {
    let root = temp_dir("lint_command", "pipe_fix_selected_import");
    let source_path = root.join("Main.terl");
    write_file(
        &source_path,
        r#"module app.Main.

import std.collections.Iterator.{map_iterator}.

/**
 * Converts a collection through a selected helper.
 */
pub collect(collection: Set, cb: Callback): List ->
    Consumer.consume(map_iterator(Set.iterator(collection), cb), "done").
"#,
    );

    assert_eq!(
        run(&[
            "--fix".to_string(),
            source_path.to_string_lossy().to_string()
        ]),
        ExitCode::SUCCESS
    );

    let fixed = fs::read_to_string(source_path).expect("fixed source");
    assert!(fixed.contains(
        "    collection\n        |> Set.iterator()\n        |> map_iterator(cb)\n        |> Consumer.consume(\"done\")."
    ));
    assert!(
        !fixed.contains("Consumer.consume(map_iterator(Set.iterator(collection), cb), \"done\").")
    );
}

/// Verifies named arguments are not offered as safe pipe rewrites.
#[test]
fn lint_rejects_named_argument_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(Set.iterator(collection), cb = cb).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "named arguments must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies named receiver-call arguments are not offered as safe pipe rewrites.
#[test]
fn lint_rejects_named_receiver_call_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, cb: Callback): Unit ->
    Iterator.each(collection.iterator(mode = "fast"), cb).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "named receiver-call arguments must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies calls with no explicit outer arguments are not rewritten.
#[test]
fn lint_rejects_default_argument_ambiguous_module_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set): Unit ->
    Iterator.each(Set.iterator(collection)).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "default-argument ambiguous module calls must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies receiver calls with no explicit outer arguments are not rewritten.
#[test]
fn lint_rejects_default_argument_ambiguous_receiver_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set): Unit ->
    Iterator.each(collection.iterator()).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "default-argument ambiguous receiver calls must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies function-value calls are not offered as safe pipe rewrites.
#[test]
fn lint_rejects_function_value_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set): Unit ->
    (make_iterator())(Set.iterator(collection)).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "function-value calls must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies unproven local callees are not offered as pipe rewrite stages.
#[test]
fn lint_rejects_unproven_local_variable_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub each(collection: Set, runner: Runner): Unit ->
    Consumer.consume(runner(Set.iterator(collection), cb), "done").
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "unproven local variable calls must not produce a pipe candidate: {diagnostics:?}"
    );
}

/// Verifies nested argument call chains are not offered as safe pipe rewrites.
#[test]
fn lint_rejects_nested_argument_pipe_candidate() {
    let diagnostics = lint_source(
        Path::new("Sample.terl"),
        r#"
module sample.

pub wrap(collection: Set, cb: Callback): Unit ->
    Wrapper.wrap(Iterator.each(Set.iterator(collection), cb)).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL1002"),
        "nested argument chains must not produce a pipe candidate: {diagnostics:?}"
    );
}
