use std::collections::BTreeMap;

use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_module_alias_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_module_alias.

	import std.collections.queue as queue.

pub len_is_zero(): Bool ->
queue.len(queue.empty()) == 0.
"#,
    )
    .expect("parse syntax output module alias fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("module alias remote call should lower directly from syntax output")
    .render();

    assert!(
        output.contains("std_collections_queue:len(std_collections_queue:empty())"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_dotted_modules_and_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module std.collections.queue.tests.

pub len_is_zero(): Bool ->
std.collections.queue.len(std.collections.queue.empty()) == 0.
"#,
    )
    .expect("parse syntax output dotted module fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("dotted modules should lower directly from syntax output")
    .render();

    assert!(output.contains("-module(std_collections_queue_tests)."));
    assert!(output.contains("std_collections_queue:len(std_collections_queue:empty())"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_colon_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_colon_remote_emit.

pub show(): Dynamic ->
io_lib:format("~p", []).
"#,
    )
    .expect("parse syntax output colon remote call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("colon remote calls should lower directly from syntax output")
    .render();

    assert!(
        output.contains("show() ->\n    io_lib:format(\"~p\", [])."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_selected_imported_functions_as_remote_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module z_dep.

pub add(x: Int): Int ->
x + 1.
"#,
    )
    .expect("parse imported function provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module a_user.

import z_dep.{add}.

pub value(): Int ->
add(1).
"#,
    )
    .expect("parse imported function consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("selected imported function should lower directly from syntax output")
    .render();

    assert!(output.contains("value() ->\n    z_dep:add(1)."));
    assert!(!output.contains("value() ->\n    add(1)."));
}

/// Verifies selected imported default arguments lower as remote call args.
///
/// Inputs:
/// - A provider module exporting a function with two defaulted parameters.
/// - A consumer importing that function and omitting the middle default while
///   supplying the final argument by name.
///
/// Output:
/// - Test passes when emitted Erlang calls the provider module function with
///   the omitted default inserted in provider declaration order.
///
/// Transformation:
/// - Builds provider interface metadata, resolves the selected import, parses
///   interface default expressions, and lowers the consumer call as a full
///   remote Erlang call.
#[test]
fn formal_syntax_output_direct_emit_inserts_selected_imported_function_default_arguments() {
    let provider = parse_module_as_syntax_output(
        r#"
module text_tools.

pub decorate(first: String, middle: String = ".", last: String = "!"): String ->
first.
"#,
    )
    .expect("parse imported default function provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module text_user.

import text_tools.{decorate}.

pub value(): String ->
decorate(first = "A", last = "?").
"#,
    )
    .expect("parse imported default function consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("selected imported function defaults should lower directly from syntax output")
    .render();

    assert!(
        output.contains("value() ->\n    text_tools:decorate(\"A\", \".\", \"?\")."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_imported_alias_constructor_subset() {
    let provider = parse_module_as_syntax_output(
        r#"
module result.

pub type Ok[T] =
{:ok, value: T}.
"#,
    )
    .expect("parse result provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module result_user.

import result.{Ok}.

pub make(value: Int): Dynamic ->
Ok(value).

pub unwrap(input: Dynamic): Dynamic ->
case input {
    Ok(value) -> value
}.
"#,
    )
    .expect("parse result consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("imported alias subset should lower directly from syntax output")
    .render();

    assert!(output.contains("make(Value) ->\n    {ok, Value}."));
    assert!(output.contains("{ok, Value} -> Value"));
    assert!(!output.contains("typer_ctor_ok"));
}
