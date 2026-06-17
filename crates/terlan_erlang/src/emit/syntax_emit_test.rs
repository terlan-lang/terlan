use std::collections::BTreeMap;

use super::test_support::test_core_module_for_syntax;
use super::try_emit_core_module_to_erlang_with_syntax_bridge;
use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::{parse_interface_module_as_syntax_output, parse_module_as_syntax_output};

/// Verifies release traversal APIs lower through syntax-bridge intrinsics.
///
/// Inputs:
/// - A syntax-output module that calls `std.collections.List.iterator(values)` and
///   passes the result to `std.collections.Iterator.next(...)`.
///
/// Output:
/// - Test assertion over the generated Erlang module text.
///
/// Transformation:
/// - Exercises the current formal syntax-output bridge from source-shaped
///   release API calls into compiler-owned traversal intrinsics, proving the
///   emitted Erlang uses explicit state-passing traversal instead of backend
///   module calls named after Terlan std modules.
#[test]
fn formal_syntax_output_direct_emit_lowers_release_traversal_intrinsics() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_traversal_emit.

pub demo(values: List): Option ->
    std.collections.Iterator.next(std.collections.List.iterator(values)).
"#,
    )
    .expect("parse traversal intrinsic bridge fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("release traversal APIs should lower through syntax bridge")
    .render();

    assert!(output.contains("demo(Values) ->"));
    assert!(output.contains("case Values of"));
    assert!(output.contains(
        "[_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}}"
    ));
    assert!(output.contains("[] -> 'none'"));
    assert!(
        !output.contains("std_collections_iterator"),
        "release traversal API must not lower to a backend module call:\n{}",
        output
    );
}

/// Verifies selected `Enumerable.map` calls lower through the std collection bridge.
///
/// Inputs:
/// - A syntax-output module importing `std.collections.Enumerable.{Enumerable}`.
/// - Embedded `std.collections.Enumerable` and `std.collections.List`
///   interface summaries.
///
/// Output:
/// - Test passes when the syntax bridge emits a BEAM `lists:map/2` call.
///
/// Transformation:
/// - Parses the selected-import trait call through the same CoreIR-gated
///   syntax bridge used by `terlc test`, proving the release collection trait
///   bridge is reachable from external modules.
#[test]
fn core_module_syntax_bridge_lowers_imported_enumerable_map() {
    let module = parse_module_as_syntax_output(
        r#"
module enumerable_map_bridge_emit.

import std.collections.Enumerable.{Enumerable}.
import std.collections.List.

pub demo(values: List[Int]): List[Int] ->
    Enumerable.map(values, (value) -> value + 1).
"#,
    )
    .expect("parse enumerable map bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let enumerable = parse_interface_module_as_syntax_output(include_str!(
        "../../../../std/summaries/std.collections.Enumerable.typi"
    ))
    .expect("parse Enumerable summary");
    let list = parse_interface_module_as_syntax_output(include_str!(
        "../../../../std/summaries/std.collections.List.typi"
    ))
    .expect("parse List summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.collections.Enumerable".to_string(),
        syntax_module_output_to_interface(&enumerable),
    );
    interfaces.insert(
        "std.collections.List".to_string(),
        syntax_module_output_to_interface(&list),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Enumerable.map should lower through syntax bridge");

    assert!(output.contains("lists:map("), "output:\n{}", output);
}

/// Verifies direct syntax-output emission recognizes `std.io.Console.println`.
///
/// Inputs:
/// - A syntax-output module with a remote `std.io.Console.println` call.
///
/// Output:
/// - Test assertion over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the syntax bridge emitter used by the current test runner
///   and verifies the portable source API lowers to backend-owned BEAM IO
///   instead of a user-visible remote Erlang module call.
#[test]
fn formal_syntax_output_direct_emit_lowers_console_println_runtime_capability() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_console_emit.

pub demo(): Unit ->
std.io.Console.println("hello").
"#,
    )
    .expect("parse console runtime capability fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("console runtime capability should lower directly from syntax output")
    .render();

    assert!(
        output.contains("demo() ->\n    begin io:format(\"~ts~n\", [\"hello\"]), unit end."),
        "output:\n{}",
        output
    );
}

/// Verifies direct syntax-output emission lowers type introspection intrinsics.
///
/// Inputs:
/// - A syntax-output module using implicit `type_of`, implicit `is_type`, and
///   `Unit` as the unit value.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the transitional syntax bridge used by `terlc build` and checks
///   that compiler-owned type introspection does not leak as raw Erlang calls
///   or variables.
#[test]
fn formal_syntax_output_direct_emit_lowers_type_introspection_intrinsics() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_type_intrinsic_emit.

pub check(): Bool ->
    is_type(1, Int).

pub kind(): Type ->
    type_of("hello").

pub main(): Unit ->
    Unit.
"#,
    )
    .expect("parse type introspection intrinsic fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("type introspection should lower directly from syntax output")
    .render();

    assert!(
        output.contains("check() ->\n    'terlan_type_int' =:= 'terlan_type_int'."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("kind() ->\n    'terlan_type_string'."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("main() ->\n    unit."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_literal_alias_constructor_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_constructor_emit.

pub type Ok[T] =
{:ok, value: T}.

pub make(value: Int): Dynamic ->
Ok(value).

pub type None =
:none.
"#,
    )
    .expect("parse alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();
    assert!(
        output.contains("-type ok(T) :: {ok, T}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("-type none() :: 'none'."));
    assert!(output.contains("make(Value) ->\n    {ok, Value}."));
    assert!(!output.contains("typer_ctor_ok"));
    assert!(!output.contains("typer_ctor_none"));
}

/// Verifies formal syntax-output emission lowers calls through function
/// parameters as Erlang fun invocations.
///
/// Inputs:
/// - A module with a lowercase function-valued parameter `f`.
/// - A body expression that invokes `f.(value)`.
///
/// Output:
/// - Test passes when emitted Erlang uses the parameter value `(F)(Value)`
///   instead of a local function call `f(Value)`.
///
/// Transformation:
/// - Parses formal syntax output, lowers it through the direct syntax-output
///   Erlang bridge, and inspects the rendered BEAM source.
#[test]
fn formal_syntax_output_direct_emit_lowers_function_value_invocation() {
    let module = parse_module_as_syntax_output(
        r#"
module function_value_invocation_emit.

pub apply(value: Int, f: (Int) -> Int): Int ->
f.(value).
"#,
    )
    .expect("parse function-value invocation emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();
    assert!(output.contains("apply(Value, F) ->\n    (F)(Value)."));
    assert!(!output.contains("f(Value)"));
}

#[test]
fn formal_syntax_output_direct_emit_rejects_unknown_uppercase_call_heads() {
    let module = parse_module_as_syntax_output(
        r#"
module unknown_constructor_emit.

pub make(value: Dynamic): Dynamic ->
Missing(value).
"#,
    )
    .expect("parse unknown constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "unresolved uppercase call heads should not lower as plain Erlang calls"
    );
}
