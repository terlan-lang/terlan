use std::collections::BTreeMap;

use super::test_support::{add_interface_summary, test_core_module_for_syntax};
use super::try_emit_core_module_to_erlang_with_syntax_bridge;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies `std.beam.Port` source calls lower to BEAM external port operations.
///
/// Inputs:
/// - A Terlan source module importing `std.beam.Port`, `std.beam.Bytes`, and
///   `std.beam.Timeout`.
/// - Generated interface summaries for the BEAM primitive contracts and their
///   standard `Result`, `Error`, and `Unit` dependencies.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Exercises the CoreIR-gated syntax bridge for external process source
///   shapes before executable epmd parity tests depend on them.
#[test]
fn core_module_syntax_bridge_lowers_beam_port_primitives() {
    let module = parse_module_as_syntax_output(
        r#"
module beam_port_bridge_emit.

import std.beam.Bytes.
import std.beam.Port.
import std.beam.Timeout.
import type std.beam.Bytes.Bytes.
import type std.beam.Port.Port.
import type std.beam.Timeout.Timeout.
import type std.core.Error.Error.
import type std.core.Result.Result.
import type std.core.Unit.Unit.

pub struct EnvVar {
    key: String,
    value: String
}.

pub struct Command {
    executable: String,
    arguments: List[String],
    environment: List[EnvVar]
}.

pub cat_command(): Command ->
    Command(
        executable = "/bin/cat",
        arguments = [],
        environment = [EnvVar(key = "TERLAN_PORT_TEST", value = "1")]
    ).

pub open_cat(): Result[Port, Error] ->
    Port.open(cat_command()).

pub write_frame(port: Port): Result[Unit, Error] ->
    port.write(Bytes.from_list([112, 105, 110, 103])).

pub read_frame(port: Port): Result[Bytes, Error] ->
    port.read(4, Timeout.milliseconds(1000)).

pub read_timeout(port: Port): Result[Bytes, Error] ->
    port.read(4, Timeout.milliseconds(1)).

pub exit_status_command(): Command ->
    Command(executable = "/bin/sh", arguments = ["-c", "exit 7"], environment = []).

pub invalid_command(): Result[Port, Error] ->
    Port.open(Command(executable = "/definitely/not/terlan-port-test", arguments = [], environment = [])).

pub close_cleanup(port: Port): Unit ->
    port.close();
    port.close().
"#,
    )
    .expect("parse BEAM Port bridge fixture");
    let core = test_core_module_for_syntax(&module);
    let mut interfaces = BTreeMap::new();
    add_interface_summary(
        &mut interfaces,
        "std.beam.Bytes",
        include_str!("../../../../../../std/summaries/std.beam.Bytes.typi"),
    );
    add_interface_summary(
        &mut interfaces,
        "std.beam.Port",
        include_str!("../../../../../../std/summaries/std.beam.Port.typi"),
    );
    add_interface_summary(
        &mut interfaces,
        "std.beam.Timeout",
        include_str!("../../../../../../std/summaries/std.beam.Timeout.typi"),
    );
    add_interface_summary(
        &mut interfaces,
        "std.core.Error",
        include_str!("../../../../../../std/summaries/std.core.Error.typi"),
    );
    add_interface_summary(
        &mut interfaces,
        "std.core.Result",
        include_str!("../../../../../../std/summaries/std.core.Result.typi"),
    );
    add_interface_summary(
        &mut interfaces,
        "std.core.Unit",
        include_str!("../../../../../../std/summaries/std.core.Unit.typi"),
    );

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("BEAM Port primitives should lower through syntax bridge");

    assert!(
        output.contains("erlang:open_port({spawn_executable, erlang:binary_to_list"),
        "Port.open should lower to BEAM open_port:\n{}",
        output
    );
    assert!(
        output.contains("exit_status")
            && output.contains("use_stdio")
            && output.contains("stderr_to_stdout")
            && output.contains("{env,"),
        "Port.open should request stdio, exit status, and environment options:\n{}",
        output
    );
    assert!(
        output.contains("Port ! {self(), {command, erlang:list_to_binary([112, 105, 110, 103])}}"),
        "Port.write should lower to a BEAM port command with a Bytes payload:\n{}",
        output
    );
    assert!(
        output.contains("{Port, {data, Data}} -> {ok, binary:part")
            && output.contains("after 1000")
            && output.contains("after 1"),
        "Port.read should lower data reads and finite timeout shapes:\n{}",
        output
    );
    assert!(
        output.contains("{Port, {exit_status, Status}}")
            && output.contains("#error{code = port_exited"),
        "Port.read should normalize process exit status into Error:\n{}",
        output
    );
    assert!(
        output.contains("#error{code = port_open_failed")
            && output.contains("#error{code = invalid_command")
            && output.contains("#error{code = port_write_failed")
            && output.contains("#error{code = timeout"),
        "Port failures should normalize into std.core.Error records:\n{}",
        output
    );
    assert!(
        output.matches("erlang:port_close(Port)").count() >= 2,
        "double close source shape should lower both close calls:\n{}",
        output
    );
    assert!(
        !output.contains("std_beam_port:open")
            && !output.contains("std_beam_port:write")
            && !output.contains("std_beam_port:read")
            && !output.contains("std_beam_port:close"),
        "BEAM Port operations must lower through compiler-owned intrinsics:\n{}",
        output
    );
}
