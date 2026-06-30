use std::collections::BTreeMap;

use super::test_support::test_core_module_for_syntax;
use super::try_emit_core_module_to_erlang_with_syntax_bridge;
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Verifies trait impls over imported types export qualified wrapper aliases.
///
/// Inputs:
/// - A provider syntax-output module importing `std.core.Option.Option`.
/// - A public `Functor[Option] for Option` implementation.
/// - The generated `std.core.Option` interface summary.
///
/// Output:
/// - Test passes when emitted Erlang exports both the provider-local wrapper
///   (`option`) and provider-qualified wrapper (`std_core_option_option`).
///
/// Transformation:
/// - Exercises trait-wrapper lowering at the provider boundary so downstream
///   modules can keep type identity qualified in `.typi` summaries while the
///   provider still supports its local source spelling.
#[test]
fn formal_syntax_output_emit_exports_qualified_trait_impl_wrapper_aliases() {
    let module = parse_module_as_syntax_output(
        r#"
module hkt_provider_emit.

import std.core.Option.{None, Option, Some}.

pub trait Functor[F[_]] {
    map[A, B](value: F[A], f: (A) -> B): F[B].
}.

pub impl Functor[Option] for Option {
    map(value: Option[A], f: (A) -> B): Option[B] ->
        case value {
            None -> None;
            Some(x) -> Some(f(x))
        }.
}.
"#,
    )
    .expect("parse imported-type HKT impl fixture");
    let option = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Option.typi"
    ))
    .expect("parse Option summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.core.Option".to_string(),
        syntax_module_output_to_interface(&option),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("imported-type trait impl should lower")
    .render();

    assert!(
        output.contains("typer_trait_functor_map_option_dict/3"),
        "local wrapper export missing:\n{}",
        output
    );
    assert!(
        output.contains("typer_trait_functor_map_std_core_option_option_dict/3"),
        "qualified wrapper export missing:\n{}",
        output
    );
}

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

/// Verifies nested binary expressions keep source grouping in Erlang output.
///
/// Inputs:
/// - A syntax-output module whose midpoint arithmetic requires nested
///   subtraction before integer division.
///
/// Output:
/// - Test passes when the emitted Erlang keeps parentheses around the nested
///   binary operands.
///
/// Transformation:
/// - Exercises syntax-bridge expression emission so backend rendering cannot
///   flatten `low + ((high - low) div 2)` into a different arithmetic tree.
#[test]
fn formal_syntax_output_emit_preserves_nested_binary_grouping() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_binary_grouping_emit.

pub midpoint(low: Int, high: Int): Int ->
    low + ((high - low) div 2).
"#,
    )
    .expect("parse binary grouping fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("binary grouping fixture should lower")
    .render();

    assert!(
        output.contains("Low + ((High - Low) div 2)"),
        "nested binary expression grouping was not preserved:\n{}",
        output
    );
}

/// Verifies receiver methods on values destructured from case patterns lower normally.
///
/// Inputs:
/// - A syntax-output module that matches `String.split_once` with
///   `Some({_local, domain})` and calls `domain.contains(".")`.
///
/// Output:
/// - Test passes when the branch body uses the string intrinsic instead of an
///   Erlang remote call named after the pattern variable.
///
/// Transformation:
/// - Exercises clause-local pattern binding and narrow primitive return-type
///   propagation for `Option[{String, String}]`.
#[test]
fn formal_syntax_output_direct_emit_lowers_pattern_bound_receiver_methods() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_pattern_receiver_emit.

import std.core.Option.{None, Some}.

pub has_domain(value: String): Bool ->
    case value.split_once("@") {
        Some({_local, domain}) ->
            domain.contains(".");
        None ->
            false
    }.
"#,
    )
    .expect("parse pattern-bound receiver fixture");
    let option = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Option.typi"
    ))
    .expect("parse Option summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.core.Option".to_string(),
        syntax_module_output_to_interface(&option),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("pattern-bound receiver method should lower")
    .render();

    assert!(
        output.contains("case string:find(Domain, \".\") of"),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains(":contains"),
        "pattern binding must not lower as a remote contains call:\n{}",
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
        "../../../../../../std/summaries/std.collections.Enumerable.typi"
    ))
    .expect("parse Enumerable summary");
    let list = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.List.typi"
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

/// Verifies selected module imports expose public constructor shorthand.
///
/// Inputs:
/// - A syntax-output module importing `std.collections.List`.
/// - The generated `std.collections.List` interface summary.
///
/// Output:
/// - Test passes when `List(1, 2)` lowers directly to the BEAM list
///   representation.
///
/// Transformation:
/// - Exercises primary-module constructor import lookup while ensuring the
///   pure std list varargs constructor does not emit a remote helper call that
///   executable release bundles do not ship.
#[test]
fn core_module_syntax_bridge_lowers_imported_list_constructor_shorthand() {
    let module = parse_module_as_syntax_output(
        r#"
module list_constructor_bridge_emit.

import std.collections.List.

pub demo(): List[Int] ->
    List(1, 2).
"#,
    )
    .expect("parse List constructor bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let list = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.List.typi"
    ))
    .expect("parse List summary");
    let mut interfaces = BTreeMap::new();
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
    .expect("List constructor shorthand should lower through syntax bridge");

    assert!(
        output.contains("demo() ->\n    [1, 2]."),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("std_collections_list:typer_ctor_list_varargs_0"),
        "output:\n{}",
        output
    );
}

/// Verifies pure `std.beam.Bytes` and `std.beam.Timeout` source behavior lowers
/// through compiler-owned BEAM primitive intrinsics.
///
/// Inputs:
/// - A Terlan source module importing `std.beam.Bytes` and `std.beam.Timeout`.
/// - Byte buffers built from list literals, converted back to lists, measured,
///   and concatenated.
/// - Finite and forever timeout constructors returned as typed timeout values.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the CoreIR-gated syntax bridge for BEAM primitive contracts so
///   binary protocol tests can use typed Terlan source without emitting calls
///   to nonexistent backend std modules.
#[test]
fn core_module_syntax_bridge_lowers_beam_bytes_and_timeout_primitives() {
    let module = parse_module_as_syntax_output(
        r#"
module beam_primitive_bridge_emit.

import std.beam.Bytes.
import std.beam.Timeout.
import type std.beam.Timeout.Timeout.

pub bytes_roundtrip(): List[Int] ->
    let bytes = Bytes.from_list([0, 1, 127, 255]);
    Bytes.to_list(bytes).

pub bytes_length(): Int ->
    let bytes = Bytes.from_list([10, 20, 30]);
    Bytes.length(bytes).

pub bytes_concat(): List[Int] ->
    let left = Bytes.from_list([1, 2]);
        right = Bytes.from_list([3, 4]);
        joined = Bytes.concat(left, right);
    Bytes.to_list(joined).

pub finite_timeout(): Timeout ->
    Timeout.milliseconds(25).

pub forever_timeout(): Timeout ->
    Timeout.forever().
"#,
    )
    .expect("parse BEAM primitive bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let bytes = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.beam.Bytes.typi"
    ))
    .expect("parse Bytes summary");
    let timeout = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.beam.Timeout.typi"
    ))
    .expect("parse Timeout summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.beam.Bytes".to_string(),
        syntax_module_output_to_interface(&bytes),
    );
    interfaces.insert(
        "std.beam.Timeout".to_string(),
        syntax_module_output_to_interface(&timeout),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("BEAM Bytes and Timeout primitives should lower through syntax bridge");

    assert!(
        output.contains("erlang:list_to_binary([0, 1, 127, 255])"),
        "Bytes.from_list should lower to BEAM binary construction:\n{}",
        output
    );
    assert!(
        output.contains("erlang:binary_to_list("),
        "Bytes.to_list should lower to BEAM binary unpacking:\n{}",
        output
    );
    assert!(
        output.contains("erlang:byte_size("),
        "Bytes.length should lower to BEAM byte_size:\n{}",
        output
    );
    assert!(
        output.contains("<<") && output.contains("/binary"),
        "Bytes.concat should lower to BEAM binary concatenation:\n{}",
        output
    );
    assert!(
        output.contains("finite_timeout() ->\n    25."),
        "Timeout.milliseconds should lower to a finite BEAM timeout:\n{}",
        output
    );
    assert!(
        output.contains("forever_timeout() ->\n    infinity."),
        "Timeout.forever should lower to the BEAM infinity timeout:\n{}",
        output
    );
    assert!(
        !output.contains("std_beam_bytes:") && !output.contains("std_beam_timeout:"),
        "BEAM Bytes/Timeout operations must lower through compiler-owned intrinsics:\n{}",
        output
    );
}

/// Verifies `std.beam.Tcp` source calls lower to passive `gen_tcp` operations.
///
/// Inputs:
/// - A Terlan source module importing `std.beam.Tcp`, `std.beam.Bytes`, and
///   `std.beam.Timeout`.
/// - Generated interface summaries for the BEAM primitive contracts and their
///   standard `Result`, `Error`, and `Unit` dependencies.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the CoreIR-gated syntax bridge for TCP daemon-test source
///   shapes before executable epmd parity tests depend on them.
#[test]
fn core_module_syntax_bridge_lowers_beam_tcp_primitives() {
    let module = parse_module_as_syntax_output(
        r#"
module beam_tcp_bridge_emit.

import std.beam.Bytes.
import std.beam.Tcp.
import std.beam.Timeout.
import type std.beam.Bytes.Bytes.
import type std.beam.Tcp.TcpSocket.
import type std.beam.Timeout.Timeout.
import type std.core.Error.Error.
import type std.core.Result.Result.
import type std.core.Unit.Unit.

pub connect_echo_peer(port: Int): Result[TcpSocket, Error] ->
    Tcp.connect("127.0.0.1", port, Timeout.milliseconds(1000)).

pub send_frame(socket: TcpSocket): Result[Unit, Error] ->
    socket.send(Bytes.from_list([1, 2, 3, 255])).

pub receive_frame(socket: TcpSocket): Result[Bytes, Error] ->
    socket.receive(4, Timeout.milliseconds(1000)).

pub refused_connection(): Result[TcpSocket, Error] ->
    Tcp.connect("127.0.0.1", 1, Timeout.milliseconds(25)).

pub receive_timeout(socket: TcpSocket): Result[Bytes, Error] ->
    socket.receive(4, Timeout.milliseconds(1)).

pub close_twice(socket: TcpSocket): Unit ->
    socket.close();
    socket.close().
"#,
    )
    .expect("parse BEAM TCP bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let bytes = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.beam.Bytes.typi"
    ))
    .expect("parse Bytes summary");
    let tcp = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.beam.Tcp.typi"
    ))
    .expect("parse Tcp summary");
    let timeout = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.beam.Timeout.typi"
    ))
    .expect("parse Timeout summary");
    let error = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Error.typi"
    ))
    .expect("parse Error summary");
    let result = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Result.typi"
    ))
    .expect("parse Result summary");
    let unit = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Unit.typi"
    ))
    .expect("parse Unit summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.beam.Bytes".to_string(),
        syntax_module_output_to_interface(&bytes),
    );
    interfaces.insert(
        "std.beam.Tcp".to_string(),
        syntax_module_output_to_interface(&tcp),
    );
    interfaces.insert(
        "std.beam.Timeout".to_string(),
        syntax_module_output_to_interface(&timeout),
    );
    interfaces.insert(
        "std.core.Error".to_string(),
        syntax_module_output_to_interface(&error),
    );
    interfaces.insert(
        "std.core.Result".to_string(),
        syntax_module_output_to_interface(&result),
    );
    interfaces.insert(
        "std.core.Unit".to_string(),
        syntax_module_output_to_interface(&unit),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("BEAM Tcp primitives should lower through syntax bridge");

    assert!(
        output.contains("gen_tcp:connect(erlang:binary_to_list(\"127.0.0.1\"), Port"),
        "Tcp.connect should lower to passive BEAM TCP connect:\n{}",
        output
    );
    assert!(
        output.contains("[binary, {packet, 0}, {active, false}]"),
        "Tcp.connect should request passive binary sockets:\n{}",
        output
    );
    assert!(
        output.contains("gen_tcp:send(Socket, erlang:list_to_binary([1, 2, 3, 255]))"),
        "Tcp.send should lower to gen_tcp:send with a Bytes payload:\n{}",
        output
    );
    assert!(
        output.contains("gen_tcp:recv(Socket, 4, 1000)")
            && output.contains("gen_tcp:recv(Socket, 4, 1)"),
        "Tcp.receive should lower finite-timeout receive shapes:\n{}",
        output
    );
    assert!(
        output.contains("refused_connection() ->")
            && output.contains("gen_tcp:connect(erlang:binary_to_list(\"127.0.0.1\"), 1"),
        "Refused connection source shape should lower through Tcp.connect:\n{}",
        output
    );
    assert!(
        output.contains("#error{code = tcp_connect_failed")
            && output.contains("#error{code = tcp_send_failed")
            && output.contains("#error{code = tcp_receive_failed"),
        "TCP failures should normalize into std.core.Error records:\n{}",
        output
    );
    assert!(
        output.matches("gen_tcp:close(Socket)").count() >= 2,
        "double close source shape should lower both close calls:\n{}",
        output
    );
    assert!(
        !output.contains("std_beam_tcp:"),
        "BEAM Tcp operations must lower through compiler-owned intrinsics:\n{}",
        output
    );
}

/// Verifies selected module imports expose public Set constructor shorthand.
///
/// Inputs:
/// - A syntax-output module importing `std.collections.Set`.
/// - The generated `std.collections.Set` interface summary.
///
/// Output:
/// - Test passes when `Set(1, 1, 2)` lowers to the imported constructor wrapper
///   and the constructor body delegates to `Set.from_list`.
///
/// Transformation:
/// - Exercises primary-module constructor import lookup for map-backed sets so
///   ergonomic source constructors preserve the compiler-owned set
///   representation boundary.
#[test]
fn core_module_syntax_bridge_lowers_imported_set_constructor_shorthand() {
    let module = parse_module_as_syntax_output(
        r#"
module set_constructor_bridge_emit.

import std.collections.Set.

pub demo(): Set[Int] ->
    Set(1, 1, 2).
"#,
    )
    .expect("parse Set constructor bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let set = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.Set.typi"
    ))
    .expect("parse Set summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.collections.Set".to_string(),
        syntax_module_output_to_interface(&set),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Set constructor shorthand should lower through syntax bridge");

    assert!(
        output.contains("std_collections_set:typer_ctor_set_varargs_0([1, 1, 2])"),
        "output:\n{}",
        output
    );
}

/// Verifies selected module imports expose public Map constructor shorthand.
///
/// Inputs:
/// - A syntax-output module importing `std.collections.Map`.
/// - The generated `std.collections.Map` interface summary.
///
/// Output:
/// - Test passes when `Map({"alice", 1}, {"bob", 2})` lowers to the imported
///   constructor wrapper.
///
/// Transformation:
/// - Exercises primary-module constructor import lookup for map-backed
///   key-value entries so ergonomic source constructors preserve the
///   compiler-owned map representation boundary.
#[test]
fn core_module_syntax_bridge_lowers_imported_map_constructor_shorthand() {
    let module = parse_module_as_syntax_output(
        r#"
module map_constructor_bridge_emit.

import std.collections.Map.

pub demo(): Map[String, Int] ->
    Map({"alice", 1}, {"bob", 2}).
"#,
    )
    .expect("parse Map constructor bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let map = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.collections.Map.typi"
    ))
    .expect("parse Map summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.collections.Map".to_string(),
        syntax_module_output_to_interface(&map),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Map constructor shorthand should lower through syntax bridge");

    assert!(
        output.contains(
            "std_collections_map:typer_ctor_map_varargs_0([{\"alice\", 1}, {\"bob\", 2}])"
        ),
        "output:\n{}",
        output
    );
}

/// Verifies `std.core.Object` constructor shorthand lowers to BEAM maps.
///
/// Inputs:
/// - A syntax-output module importing `std.core.Object`.
/// - The generated `std.core.Object` interface summary.
///
/// Output:
/// - Test passes when `Object({"status", "ok"})` lowers to
///   `maps:from_list/1`.
///
/// Transformation:
/// - Proves `Object[V]` stays a source-level semantic alias over
///   `Map[String, V]` and does not require a generated Erlang wrapper module.
#[test]
fn core_module_syntax_bridge_lowers_imported_object_constructor_shorthand_to_map() {
    let module = parse_module_as_syntax_output(
        r#"
module object_constructor_bridge_emit.

import std.core.Object.

pub demo(): Object[String] ->
    Object({"status", "ok"}).
"#,
    )
    .expect("parse Object constructor bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let object = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Object.typi"
    ))
    .expect("parse Object summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.core.Object".to_string(),
        syntax_module_output_to_interface(&object),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Object constructor shorthand should lower through syntax bridge");

    assert!(
        output.contains("maps:from_list([{\"status\", \"ok\"}])"),
        "output:\n{}",
        output
    );
}

/// Verifies `std.core.Object` receiver methods lower through map intrinsics.
///
/// Inputs:
/// - A syntax-output module importing `std.core.Object`.
/// - The generated `std.core.Object` interface summary.
///
/// Output:
/// - Test passes when `payload.size()` lowers to `maps:size/1`.
///
/// Transformation:
/// - Keeps the Object receiver API tied to the same BEAM map backing as
///   `std.collections.Map` without exposing a separate runtime module.
#[test]
fn core_module_syntax_bridge_lowers_object_receiver_method_to_map_intrinsic() {
    let module = parse_module_as_syntax_output(
        r#"
module object_receiver_bridge_emit.

import std.core.Object.

pub demo(payload: Object[String]): Int ->
    payload.size().
"#,
    )
    .expect("parse Object receiver bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let object = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Object.typi"
    ))
    .expect("parse Object summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.core.Object".to_string(),
        syntax_module_output_to_interface(&object),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("Object receiver method should lower through syntax bridge");

    assert!(output.contains("maps:size(Payload)"), "output:\n{}", output);
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

/// Verifies JSON receiver methods lower to the SafeNative std module ABI.
///
/// Inputs:
/// - A syntax-output module with an imported `std.data.Json.Json` parameter and
///   representative receiver-method calls.
///
/// Output:
/// - Test assertions over the rendered Erlang remote calls.
///
/// Transformation:
/// - Exercises the direct syntax bridge used by package builds so
///   `json.get(...)` and `value.as_string()` lower like module-style
///   `Json.get(json, ...)` calls instead of being treated as local methods.
#[test]
fn formal_syntax_output_direct_emit_lowers_json_receiver_methods() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_json_receiver_emit.

import type std.core.Result.Result.
import type std.data.Json.{Json, JsonError}.

pub field(json: Json): Result[Json, JsonError] ->
    json.get("name").

pub name(value: Json): Result[String, JsonError] ->
    value.as_string().
"#,
    )
    .expect("parse JSON receiver fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("JSON receiver methods should lower directly from syntax output")
    .render();

    assert!(
        output.contains("std_data_json:get(Json, \"name\")"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("std_data_json:as_string(Value)"),
        "output:\n{}",
        output
    );
}

/// Verifies HTTP request receiver accessors lower to BEAM handler request maps.
///
/// Inputs:
/// - A syntax-output module with an imported `std.http.Request.Request`
///   parameter and representative `std.http.Request` receiver calls.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the direct syntax bridge used by `terlc serve` handlers and
///   proves request accessors read the BEAM request-map fields instead of
///   emitting calls to non-existent backend std modules.
#[test]
fn formal_syntax_output_direct_emit_lowers_http_request_accessors() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_http_request_emit.

import type std.http.Request.Request.
import type std.core.Option.Option.
import type std.core.Result.Result.
import type std.data.Json.Json.
import type std.core.Error.Error.

pub method_name(request: Request): String ->
    request.method().

pub route_path(request: Request): String ->
    request.path().

pub request_body(request: Request): String ->
    request.body_text().

pub request_json(request: Request): Result[Json, Error] ->
    request.body_json().

pub route_id(request: Request): Option[String] ->
    request.param("id").

pub page(request: Request): Option[String] ->
    request.query(name = "page").

pub accept(request: Request): Option[String] ->
    request.header("accept").

pub session(request: Request): Option[String] ->
    request.cookie("session").

pub raw_query(request: Request): String ->
    request.query_string().
"#,
    )
    .expect("parse HTTP request accessor fixture");
    let error = parse_interface_module_as_syntax_output(include_str!(
        "../../../../../../std/summaries/std.core.Error.typi"
    ))
    .expect("parse Error summary");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        "std.core.Error".to_string(),
        syntax_module_output_to_interface(&error),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("HTTP request accessors should lower directly from syntax output")
    .render();

    for required_lookup in [
        "method_name(Request) ->\n    maps:get(method, Request).",
        "route_path(Request) ->\n    maps:get(path, Request).",
        "request_body(Request) ->\n    maps:get(body, Request).",
        "raw_query(Request) ->\n    maps:get(query_string, Request).",
    ] {
        assert!(
            output.contains(required_lookup),
            "missing required request lookup `{required_lookup}` in output:\n{output}"
        );
    }
    for optional_map in [
        "maps:get(params, Request, #{})",
        "maps:get(query, Request, #{})",
        "maps:get(headers, Request, #{})",
        "maps:get(cookies, Request, #{})",
    ] {
        assert!(
            output.contains(optional_map),
            "missing optional request map `{optional_map}` in output:\n{output}"
        );
    }
    assert!(
        output.contains("unicode:characters_to_binary(\"id\")"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("unicode:characters_to_binary(\"page\")"),
        "named request query argument should lower to the query key:\n{}",
        output
    );
    assert!(
        output.contains("unicode:characters_to_binary(\"accept\")"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("unicode:characters_to_binary(\"session\")"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("try json:decode(maps:get(body, Request)) of Value -> {ok, Value}"),
        "request.body_json() should decode the buffered request body:\n{}",
        output
    );
    assert!(
        output.contains(
            "catch _:_ -> {error, #error{code = invalid_json, message = \"invalid JSON request body\"}} end"
        ),
        "request.body_json() should map decoder failures to std.core.Error:\n{}",
        output
    );
    assert!(
        output.contains("{ok, Value} -> {'some', Value}; error -> 'none'"),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("std_http_request"),
        "HTTP request accessors must not lower to backend std module calls:\n{}",
        output
    );
}

/// Verifies served-handler response builders lower to the BEAM response ABI.
///
/// Inputs:
/// - A syntax-output module that imports `std.http.Response` and calls text,
///   HTML, and redirect response builders.
///
/// Output:
/// - Test assertions over generated Erlang response tuples.
///
/// Transformation:
/// - Exercises the direct syntax bridge used by `terlc serve` handlers and
///   proves selected response builders return the stable
///   `{terlan_response, ...}` ABI consumed by the local Hyper/Tokio server.
#[test]
fn formal_syntax_output_direct_emit_lowers_http_response_builders() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_http_response_emit.

import std.http.Response.
import type std.http.Response.Response.

pub home(): Response ->
    Response.text("home").

pub created(): Response ->
    Response.text("created", 201).

pub json_text(): Response ->
    Response.json_text("{\"ok\":true}").

pub named_json_text(): Response ->
    Response.json_text(status = 202, value = "{\"accepted\":true}").

pub named_text(): Response ->
    Response.text(status = 201, value = "named").

pub page(): Response ->
    Response.html("<main>ok</main>").

pub moved(): Response ->
    Response.redirect("/login").

pub temporary(): Response ->
    Response.redirect("/temporary", 307).

pub named_redirect(): Response ->
    Response.redirect(status = 308, location = "/named").

pub accepted(response: Response): Response ->
    response.with_status(202).

pub named_status(response: Response): Response ->
    response.with_status(code = 203).

pub traced(response: Response): Response ->
    response.with_header("x-terlan", "yes").

pub named_header(response: Response): Response ->
    response.with_header(value = "yes", name = "x-named").

pub cookied(response: Response): Response ->
    response.set_cookie_header("session=abc; Path=/").

pub named_cookie(response: Response): Response ->
    response.set_cookie_header(value = "theme=dark; Path=/").

pub cookie_builder(response: Response): Response ->
    response.cookie("session", "abc", http_only = true).

pub with_cookie_builder(response: Response): Response ->
    response.with_cookie("session", "abc", http_only = true).

pub cookie_options_builder(response: Response): Response ->
    response.cookie_with_options("session", "abc", domain = "example.com", include_max_age = true, max_age = 10, same_site = "lax").

pub with_cookie_options_builder(response: Response): Response ->
    response.with_cookie_options("session", "abc", domain = "example.com", include_max_age = true, max_age = 10, same_site = "lax").

pub delete_cookie_builder(response: Response): Response ->
    response.delete_cookie(name = "session").

pub with_deleted_cookie_builder(response: Response): Response ->
    response.with_deleted_cookie(name = "session").

pub cookie_sequence(response: Response): Response ->
    response.cookie("session", "abc");
    response.

pub pipe_status(): Response ->
    Response.text("ok") |> with_status(201).

pub pipe_cookie(): Response ->
    Response.text("ok") |> cookie("session", "abc").

pub chained_status(): Response ->
    Response.text("ok").with_status(202).

pub chained_header(): Response ->
    Response.text("ok").with_header("x-terlan", "yes").

pub chained_cookie(): Response ->
    Response.text("ok").with_cookie("session", "abc").

pub parameter_named_body(body: String): Response ->
    Response.text(body).with_header("content-type", "application/json; charset=utf-8").
"#,
    )
    .expect("parse HTTP response builder fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("HTTP response builders should lower directly from syntax output")
    .render();

    for expected in [
        "home() ->\n    {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"home\")}.",
        "created() ->\n    {terlan_response, 201, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"created\")}.",
        "json_text() ->\n    {terlan_response, 200, <<\"application/json; charset=utf-8\">>, unicode:characters_to_binary(\"{\\\"ok\\\":true}\")}.",
        "named_json_text() ->\n    {terlan_response, 202, <<\"application/json; charset=utf-8\">>, unicode:characters_to_binary(\"{\\\"accepted\\\":true}\")}.",
        "named_text() ->\n    {terlan_response, 201, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"named\")}.",
        "page() ->\n    {terlan_response, 200, <<\"text/html; charset=utf-8\">>, unicode:characters_to_binary(\"<main>ok</main>\")}.",
        "moved() ->\n    {terlan_response, 302, <<\"text/plain; charset=utf-8\">>, [{<<\"Location\">>, unicode:characters_to_binary(\"/login\")}], <<>>}.",
        "temporary() ->\n    {terlan_response, 307, <<\"text/plain; charset=utf-8\">>, [{<<\"Location\">>, unicode:characters_to_binary(\"/temporary\")}], <<>>}.",
        "named_redirect() ->\n    {terlan_response, 308, <<\"text/plain; charset=utf-8\">>, [{<<\"Location\">>, unicode:characters_to_binary(\"/named\")}], <<>>}.",
        "accepted(Response) ->\n    case Response of {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, 202, _TerlanResponseContentType, _TerlanResponseBody}; {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, 202, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} end.",
        "named_status(Response) ->\n    case Response of {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, 203, _TerlanResponseContentType, _TerlanResponseBody}; {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, 203, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} end.",
        "traced(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{unicode:characters_to_binary(\"x-terlan\"), unicode:characters_to_binary(\"yes\")}], _TerlanResponseBody}; {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{unicode:characters_to_binary(\"x-terlan\"), unicode:characters_to_binary(\"yes\")}], _TerlanResponseBody} end.",
        "named_header(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{unicode:characters_to_binary(\"x-named\"), unicode:characters_to_binary(\"yes\")}], _TerlanResponseBody}; {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{unicode:characters_to_binary(\"x-named\"), unicode:characters_to_binary(\"yes\")}], _TerlanResponseBody} end.",
        "cookied(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary(\"session=abc; Path=/\")}], _TerlanResponseBody}; {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{<<\"Set-Cookie\">>, unicode:characters_to_binary(\"session=abc; Path=/\")}], _TerlanResponseBody} end.",
        "named_cookie(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary(\"theme=dark; Path=/\")}], _TerlanResponseBody}; {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{<<\"Set-Cookie\">>, unicode:characters_to_binary(\"theme=dark; Path=/\")}], _TerlanResponseBody} end.",
        "cookie_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case true of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end])}], _TerlanResponseBody};",
        "with_cookie_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case true of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end])}], _TerlanResponseBody};",
        "cookie_options_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case \"example.com\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Domain=\", \"example.com\"] end, case true of true -> [\"; Max-Age=\", integer_to_list(10)]; _ -> \"\" end, case \"\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Expires=\", \"\"] end, case false of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end, case \"lax\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; SameSite=\", \"lax\"] end])}], _TerlanResponseBody};",
        "with_cookie_options_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case \"example.com\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Domain=\", \"example.com\"] end, case true of true -> [\"; Max-Age=\", integer_to_list(10)]; _ -> \"\" end, case \"\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Expires=\", \"\"] end, case false of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end, case \"lax\" of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; SameSite=\", \"lax\"] end])}], _TerlanResponseBody};",
        "delete_cookie_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=; Path=\", \"/\", \"; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT\"])}], _TerlanResponseBody};",
        "with_deleted_cookie_builder(Response) ->\n    case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=; Path=\", \"/\", \"; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT\"])}], _TerlanResponseBody};",
        "cookie_sequence(Response) ->\n    begin\n    _TerlanMutReceiver0 = case Response of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case false of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end])}], _TerlanResponseBody};",
        "    _TerlanMutReceiver0\nend.",
        "pipe_status() ->\n    begin\n    _TerlanMutReceiver = case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"ok\")} of {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, 201, _TerlanResponseContentType, _TerlanResponseBody};",
        "pipe_cookie() ->\n    begin\n    _TerlanMutReceiver = case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"ok\")} of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case false of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end])}], _TerlanResponseBody};",
        "chained_status() ->\n    case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"ok\")} of {terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, 202, _TerlanResponseContentType, _TerlanResponseBody};",
        "chained_header() ->\n    case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"ok\")} of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{unicode:characters_to_binary(\"x-terlan\"), unicode:characters_to_binary(\"yes\")}], _TerlanResponseBody};",
        "chained_cookie() ->\n    case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(\"ok\")} of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{<<\"Set-Cookie\">>, unicode:characters_to_binary([\"session\", \"=\", \"abc\", \"; Path=\", \"/\", case false of true -> \"; HttpOnly\"; _ -> \"\" end, case false of true -> \"; Secure\"; _ -> \"\" end])}], _TerlanResponseBody};",
        "parameter_named_body(Body) ->\n    case {terlan_response, 200, <<\"text/plain; charset=utf-8\">>, unicode:characters_to_binary(Body)} of {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody} -> {terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{unicode:characters_to_binary(\"content-type\"), unicode:characters_to_binary(\"application/json; charset=utf-8\")}], _TerlanResponseBody};",
    ] {
        assert!(
            output.contains(expected),
            "missing response builder output `{expected}` in:\n{output}"
        );
    }
    assert!(
        !output.contains("std_http_response"),
        "HTTP response builders must not lower to backend std module calls:\n{}",
        output
    );
}

/// Verifies value-only `std.io.File` imports emit the FileError record.
///
/// Inputs:
/// - A syntax-output module importing `std.io.File` as a value module and
///   calling `File.read_text`.
///
/// Output:
/// - Test assertions over the generated Erlang record declaration and file
///   runtime expression.
///
/// Transformation:
/// - Exercises the import spelling used by source modules (`import std.io.File.`)
///   so runtime record construction has the local record form required by BEAM.
#[test]
fn formal_syntax_output_direct_emit_declares_file_error_for_value_file_import() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_file_emit.

import std.io.File.
import type std.core.Result.Result.

pub load(path: String): Result[String, std.io.File.FileError] ->
    File.read_text(path).
"#,
    )
    .expect("parse file read fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("file read fixture should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-record(fileerror, {code, message, path})."),
        "file read output should declare FileError record:\n{}",
        output
    );
    assert!(
        output.contains("#fileerror{code = not_found"),
        "file read output should construct FileError records:\n{}",
        output
    );
}

/// Verifies HTTP router builders lower to a BEAM-side router term.
///
/// Inputs:
/// - A syntax-output module using chained `Router.new().get(...).fallback(...)`
///   and direct `Router.get(Router.new(), ...)` forms.
///
/// Output:
/// - Test assertions over generated Erlang router table expressions.
///
/// Transformation:
/// - Exercises the direct syntax bridge used by web package builds and proves
///   router builder functions do not emit calls to non-existent backend std
///   modules while route manifests remain owned by the web build path.
#[test]
fn formal_syntax_output_direct_emit_lowers_http_router_builders() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_http_router_emit.

import std.http.Router.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.

pub router(): Router ->
    Router.new()
        .get("/", home)
        .options("/health", health)
        .fallback(not_found).

pub direct_router(): Router ->
    let router = Router.get(Router.new(), "/", home);
    Router.fallback(router, not_found).

pub home(_request: Request): Response ->
    Response.text("home").

pub health(_request: Request): Response ->
    Response.text("ok").

pub not_found(_request: Request): Response ->
    Response.text("not found").
"#,
    )
    .expect("parse HTTP router builder fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("HTTP router builders should lower directly from syntax output")
    .render();

    for expected in [
        "{terlan_router, []}",
        "Routes ++ [{get, unicode:characters_to_binary(\"/\"), fun home/1}]",
        "Routes ++ [{options, unicode:characters_to_binary(\"/health\"), fun health/1}]",
        "Routes ++ [{fallback, fun not_found/1}]",
    ] {
        assert!(
            output.contains(expected),
            "missing router builder output `{expected}` in:\n{output}"
        );
    }
    assert!(
        !output.contains("std_http_router"),
        "HTTP router builders must not lower to backend std module calls:\n{}",
        output
    );
}

/// Verifies direct syntax-output emission recognizes every `std.log` level.
///
/// Inputs:
/// - Syntax-output modules with remote `std.log` calls.
///
/// Output:
/// - Test assertion over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the syntax bridge emitter used by the current test runner
///   and verifies the portable logging API lowers to backend-owned BEAM IO
///   instead of a user-visible remote Erlang module call.
#[test]
fn formal_syntax_output_direct_emit_lowers_all_std_log_levels_to_runtime_capability() {
    for level in ["debug", "info", "warn", "error"] {
        let module = parse_module_as_syntax_output(&format!(
            r#"
module syntax_output_log_emit.

pub demo(): Unit ->
std.log.{level}("hello").
"#
        ))
        .expect("parse std.log runtime capability fixture");

        let output = super::lower_syntax_module_output(
            &module,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .expect("std.log runtime capability should lower directly from syntax output")
        .render();

        assert!(
            output.contains("demo() ->\n    begin io:format(\"~ts~n\", [\"hello\"]), unit end."),
            "std.log.{level} output:\n{}",
            output
        );
    }
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

/// Verifies formal syntax-output emission reorders alias constructor labels.
///
/// Inputs:
/// - A transparent alias `Pair = {:pair, left: Int, right: Int}`.
/// - A constructor call written as `Pair(right = 2, left = 1)`.
///
/// Output:
/// - Test passes when Erlang output uses alias field order `{'pair', 1, 2}`.
///
/// Transformation:
/// - Parses formal syntax output, lowers alias constructor arguments by source
///   tuple-field name, and inspects the rendered Erlang tuple shape.
#[test]
fn formal_syntax_output_direct_emit_reorders_alias_constructor_field_labels() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_constructor_label_emit.

pub type Pair =
{:pair, left: Int, right: Int}.

pub make(): Dynamic ->
Pair(right = 2, left = 1).
"#,
    )
    .expect("parse alias constructor label emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower alias constructor field labels")
    .render();

    assert!(
        output.contains("make() ->\n    {'pair', 1, 2}."),
        "output:\n{}",
        output
    );
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

/// Verifies named call arguments lower in declaration parameter order.
///
/// Inputs:
/// - A local function call written with out-of-order named arguments.
///
/// Output:
/// - Test passes when emitted Erlang calls the function with positional
///   arguments ordered as declared by the callee.
///
/// Transformation:
/// - Parses formal syntax output, lowers it through the direct syntax-output
///   Erlang bridge, and inspects the rendered call site.
#[test]
fn formal_syntax_output_direct_emit_reorders_local_named_call_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module named_call_argument_emit.

pub pick(first: Int, second: Int, third: Int): Int ->
    second.

pub run(): Int ->
    pick(1, third = 3, second = 2).
"#,
    )
    .expect("parse named-call emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("named arguments should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run() ->\n    pick(1, 2, 3)."),
        "named arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies omitted local function defaults lower as explicit backend args.
///
/// Inputs:
/// - A local function with a trailing defaulted parameter and a call omitting
///   that argument.
///
/// Output:
/// - Test passes when emitted Erlang calls the function with the lowered
///   default expression in the missing argument slot.
///
/// Transformation:
/// - Parses formal syntax output, resolves the local function target, and
///   expands omitted default arguments during direct Erlang lowering.
#[test]
fn formal_syntax_output_direct_emit_inserts_local_function_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module omitted_function_default_emit.

pub greet(name: String, excited: Bool = false): String ->
    name.

pub run(): String ->
    greet("Ada").
"#,
    )
    .expect("parse omitted function default fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("omitted local defaults should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run() ->\n    greet(\"Ada\", false)."),
        "omitted defaults should emit as explicit backend arguments:\n{}",
        output
    );
}

/// Verifies named explicit-constructor arguments lower in declaration order.
///
/// Inputs:
/// - A local constructor call written with out-of-order named arguments.
///
/// Output:
/// - Test passes when emitted Erlang calls the generated constructor function
///   with positional arguments ordered as declared by the constructor clause.
///
/// Transformation:
/// - Parses formal syntax output, lowers it through the direct syntax-output
///   Erlang bridge, and inspects the rendered constructor call site.
#[test]
fn formal_syntax_output_direct_emit_reorders_local_named_constructor_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module named_constructor_argument_emit.

pub type User = {id: Int, name: String, active: Bool}.

pub constructor User {
    (id: Int, name: String, active: Bool): User ->
        {id, name, active}
}.

pub run(): User ->
    User(1, active = false, name = "Ada").
"#,
    )
    .expect("parse named-constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("named constructor arguments should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run() ->\n    typer_ctor_user_3(1, \"Ada\", false)."),
        "named constructor arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies omitted constructor defaults lower in declaration order.
///
/// Inputs:
/// - A constructor with two trailing defaulted parameters.
/// - A named call that supplies the first required parameter and the final
///   defaulted parameter while omitting the middle defaulted parameter.
///
/// Output:
/// - Test passes when emitted Erlang calls the generated constructor function
///   with the omitted default inserted into the middle slot.
///
/// Transformation:
/// - Resolves the local constructor target, maps named arguments into full
///   declaration slots, and lowers omitted defaults as explicit Erlang args.
#[test]
fn formal_syntax_output_direct_emit_inserts_constructor_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module constructor_default_argument_emit.

pub type User = {id: Int, name: String, active: Bool}.

pub constructor User {
    (id: Int, name: String = "Ada", active: Bool = true): User ->
        {id, name, active}
}.

pub run(): User ->
    User(id = 1, active = false).
"#,
    )
    .expect("parse constructor default emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("constructor default arguments should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run() ->\n    typer_ctor_user_3(1, \"Ada\", false)."),
        "constructor default arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies named receiver-method arguments lower in declaration order.
///
/// Inputs:
/// - A local receiver-method call written with out-of-order named arguments.
///
/// Output:
/// - Test passes when emitted Erlang calls the receiver-first method function
///   with the receiver first and non-receiver arguments ordered as declared.
///
/// Transformation:
/// - Parses formal syntax output, lowers it through the direct syntax-output
///   Erlang bridge, and inspects the rendered receiver-method call site.
#[test]
fn formal_syntax_output_direct_emit_reorders_local_receiver_named_call_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module named_receiver_argument_emit.

pub struct User {
    name: String
}.

pub (user: User) label(prefix: String, suffix: String): String ->
    user.name.

pub run(user: User): String ->
    user.label(suffix = "!", prefix = "User").
"#,
    )
    .expect("parse named receiver emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("named receiver arguments should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run(User) ->\n    label(User, \"User\", \"!\")."),
        "named receiver arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies omitted receiver-method defaults lower as explicit backend args.
///
/// Inputs:
/// - A receiver method with a defaulted final non-receiver parameter.
/// - A call that omits that parameter.
///
/// Output:
/// - Test passes when emitted Erlang calls the receiver-first function with
///   the lowered default inserted in declaration order.
///
/// Transformation:
/// - Resolves the receiver method target, lowers supplied arguments, then fills
///   omitted declaration slots from syntax-output default expressions.
#[test]
fn formal_syntax_output_direct_emit_inserts_receiver_method_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module receiver_default_argument_emit.

pub struct User {
    name: String
}.

pub (user: User) label(prefix: String, suffix: String = "!"): String ->
    user.name.

pub run(user: User): String ->
    user.label("User").
"#,
    )
    .expect("parse receiver default emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("receiver default arguments should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run(User) ->\n    label(User, \"User\", \"!\")."),
        "receiver default arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies receiver-method pipe calls lower omitted defaults.
///
/// Inputs:
/// - A receiver method with a defaulted final non-receiver parameter.
/// - A pipe call that omits that parameter.
///
/// Output:
/// - Test passes when emitted Erlang calls the receiver-first method function
///   with the default inserted.
///
/// Transformation:
/// - Resolves the pipe target as a receiver method and reuses receiver-method
///   default argument completion during pipe lowering.
#[test]
fn formal_syntax_output_direct_emit_inserts_receiver_method_pipe_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module receiver_pipe_default_argument_emit.

pub struct User {
    name: String
}.

pub (user: User) label(prefix: String, suffix: String = "!"): String ->
    user.name.

pub run(user: User): String ->
    user |> label("User").
"#,
    )
    .expect("parse receiver pipe default emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("receiver pipe defaults should lower directly from syntax output")
    .render();

    assert!(
        output.contains("run(User) ->\n    label(User, \"User\", \"!\")."),
        "receiver pipe default arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies mutable receiver pipes lower omitted defaults.
///
/// Inputs:
/// - A mutable receiver method with a defaulted final non-receiver parameter.
/// - A pipe call that omits that parameter.
///
/// Output:
/// - Test passes when emitted Erlang updates the hidden receiver binding using
///   a receiver-first call that includes the default argument.
///
/// Transformation:
/// - Uses mutable receiver pipe lowering and fills omitted receiver-method
///   defaults before constructing the hidden updated-receiver binding.
#[test]
fn formal_syntax_output_direct_emit_inserts_mutable_receiver_pipe_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_pipe_default_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(key: String, value: String = "default"): Unit ->
    map.

pub run(map: Map): Map ->
    map |> put("a").
"#,
    )
    .expect("parse mutable receiver pipe default emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver pipe defaults should lower directly from syntax output")
    .render();

    assert!(
        output.contains("_TerlanMutReceiver = put(Map, \"a\", \"default\")"),
        "mutable receiver pipe default arguments should emit in declaration order:\n{}",
        output
    );
}

/// Verifies mutable receiver sequences lower omitted defaults.
///
/// Inputs:
/// - A mutable receiver method with a defaulted final non-receiver parameter.
/// - A semicolon-style sequence that calls the method before returning the
///   receiver.
///
/// Output:
/// - Test passes when emitted Erlang updates the hidden receiver binding using
///   the defaulted method argument.
///
/// Transformation:
/// - Reuses receiver-method default argument completion from mutable receiver
///   sequence lowering before applying the source-name replacement.
#[test]
fn formal_syntax_output_direct_emit_inserts_mutable_receiver_sequence_default_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_sequence_default_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(key: String, value: String = "default"): Unit ->
    map.

pub run(map: Map): Map ->
    map.put("a");
    map.
"#,
    )
    .expect("parse mutable receiver sequence default emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver sequence defaults should lower directly from syntax output")
    .render();

    assert!(
        output.contains("put(Map, \"a\", \"default\")"),
        "mutable receiver sequence default arguments should emit in declaration order:\n{}",
        output
    );
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

/// Verifies backend emission fails closed for unresolved constructor-like calls.
///
/// Inputs:
/// - Syntax-output source with an uppercase call head that was not resolved by
///   constructor/type analysis.
///
/// Output:
/// - Test passes when Erlang lowering returns `None`.
///
/// Transformation:
/// - Exercises the backend adversarial path directly so unresolved source
///   shapes cannot leak into generated Erlang as plain function calls.
#[test]
fn adversarial_backend_emit_rejects_unresolved_uppercase_call_heads() {
    let module = parse_module_as_syntax_output(
        r#"
module adversarial_unknown_constructor_emit.

pub make(value: Dynamic): Dynamic ->
    UnknownConstructor(value).
"#,
    )
    .expect("parse adversarial unresolved constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "unresolved uppercase call heads must fail before Erlang rendering"
    );
}

/// Verifies primitive receiver named arguments lower in intrinsic ABI order.
///
/// Inputs:
/// - A string receiver `replace` call whose named arguments are written out of
///   declaration order.
///
/// Output:
/// - Test passes when generated Erlang calls `string:replace` with pattern
///   before replacement.
///
/// Transformation:
/// - Reorders source named arguments before delegating to the primitive
///   intrinsic Erlang lowerer.
#[test]
fn formal_syntax_output_direct_emit_reorders_primitive_receiver_named_arguments() {
    let module = parse_module_as_syntax_output(
        r#"
module primitive_receiver_named_emit.

pub demo(): String ->
    "hello".replace(replacement = "x", pattern = "l").
"#,
    )
    .expect("parse primitive receiver named argument fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("primitive receiver named argument should lower")
    .render();

    assert!(
        output.contains("string:replace(\"hello\", \"l\", \"x\", 'all')"),
        "output:\n{}",
        output
    );
}

/// Verifies string-plus-int lowers to binary-safe text concatenation.
///
/// Inputs:
/// - A syntax-output module returning `"index: " + index`.
///
/// Output:
/// - Test passes when emitted Erlang converts operands through
///   `unicode:characters_to_binary`.
///
/// Transformation:
/// - Keeps the syntax bridge aligned with Terlan's string-plus-printable-scalar
///   type rule instead of leaking Erlang `string:concat/2` limitations.
#[test]
fn formal_syntax_output_direct_emit_lowers_string_concat_with_int_operand() {
    let module = parse_module_as_syntax_output(
        r#"
module string_concat_int_emit.

pub label(index: Int): String ->
    "index: " + index.
"#,
    )
    .expect("parse string concat fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("string concat with int should lower")
    .render();

    assert!(
        output.contains("unicode:characters_to_binary(["),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("integer_to_binary(V)"),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("string:concat"),
        "string-plus-scalar should not lower through string:concat/2:\n{}",
        output
    );
}
