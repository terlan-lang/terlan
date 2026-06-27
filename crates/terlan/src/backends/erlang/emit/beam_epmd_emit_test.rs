use std::collections::BTreeMap;

use super::test_support::{add_interface_summary, test_core_module_for_syntax};
use super::try_emit_core_module_to_erlang_with_syntax_bridge;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies an epmd smoke-parity source fixture lowers through BEAM primitives.
///
/// Inputs:
/// - A Terlan source module that starts epmd with `std.beam.Port`.
/// - TCP protocol helpers for ALIVE2 registration, PORT_PLEASE2 lookup, names,
///   duplicate registration, and daemon shutdown.
/// - Generated interface summaries for the BEAM primitive contracts.
///
/// Output:
/// - Test assertions over the rendered Erlang module.
///
/// Transformation:
/// - Keeps the epmd smoke shape in Terlan source before the executable runtime
///   gate exists, while verifying that all daemon/process/socket operations
///   lower through compiler-owned BEAM intrinsics.
#[test]
fn core_module_syntax_bridge_lowers_epmd_smoke_parity_shape() {
    let module = parse_module_as_syntax_output(
        r#"
module beam_epmd_smoke_emit.

import std.beam.Bytes.
import std.beam.Port.
import std.beam.Tcp.
import std.beam.Timeout.
import type std.beam.Bytes.Bytes.
import type std.beam.Port.Port.
import type std.beam.Tcp.TcpSocket.
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

pub epmd_command(executable: String, port_text: String): Command ->
    Command(
        executable = executable,
        arguments = ["-address", "127.0.0.1", "-port", port_text, "-packet_timeout", "1"],
        environment = [EnvVar(key = "TERLAN_EPMD_SMOKE", value = "1")]
    ).

pub start_epmd(executable: String, port_text: String): Result[Port, Error] ->
    Port.open(epmd_command(executable, port_text)).

pub connect_epmd(port: Int): Result[TcpSocket, Error] ->
    Tcp.connect("127.0.0.1", port, Timeout.milliseconds(1000)).

pub alive2_node_request(): Bytes ->
    Bytes.from_list([0, 17, 120, 48, 57, 77, 0, 0, 5, 0, 5, 0, 4, 102, 111, 111, 49, 0, 0]).

pub port_please2_request(): Bytes ->
    Bytes.from_list([0, 5, 122, 102, 111, 111, 49]).

pub names_request(): Bytes ->
    Bytes.from_list([0, 1, 110]).

pub kill_request(): Bytes ->
    Bytes.from_list([0, 1, 107]).

pub register_alive(socket: TcpSocket): Result[Bytes, Error] ->
    socket.send(alive2_node_request());
    socket.receive(4, Timeout.milliseconds(1000)).

pub lookup_registered(socket: TcpSocket): Result[Bytes, Error] ->
    socket.send(port_please2_request());
    socket.receive(64, Timeout.milliseconds(1000)).

pub list_names(socket: TcpSocket): Result[Bytes, Error] ->
    socket.send(names_request());
    socket.receive(256, Timeout.milliseconds(1000)).

pub duplicate_registration(socket: TcpSocket): Result[Bytes, Error] ->
    socket.send(alive2_node_request());
    socket.receive(4, Timeout.milliseconds(1000)).

pub shutdown_epmd(socket: TcpSocket, daemon: Port): Unit ->
    socket.send(kill_request());
    socket.close();
    daemon.close().
"#,
    )
    .expect("parse EPMD smoke bridge fixture");
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
        "std.beam.Tcp",
        include_str!("../../../../../../std/summaries/std.beam.Tcp.typi"),
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
    .expect("EPMD smoke fixture should lower through syntax bridge");

    assert!(
        output.contains("erlang:open_port({spawn_executable")
            && output.contains("\"-packet_timeout\"")
            && output.contains("\"TERLAN_EPMD_SMOKE\""),
        "epmd daemon startup should lower through Port.open:\n{}",
        output
    );
    assert!(
        output.contains("gen_tcp:connect(erlang:binary_to_list(\"127.0.0.1\"), Port")
            && output.contains("[binary, {packet, 0}, {active, false}]"),
        "epmd client connection should lower through passive Tcp.connect:\n{}",
        output
    );
    assert!(
        output.contains("erlang:list_to_binary([0, 17, 120")
            && output.contains("erlang:list_to_binary([0, 5, 122")
            && output.contains("erlang:list_to_binary([0, 1, 110")
            && output.contains("erlang:list_to_binary([0, 1, 107"),
        "epmd protocol frames should lower as byte buffers:\n{}",
        output
    );
    assert!(
        output.matches("gen_tcp:send(Socket,").count() >= 5
            && output.contains("gen_tcp:recv(Socket, 4, 1000)")
            && output.contains("gen_tcp:recv(Socket, 64, 1000)")
            && output.contains("gen_tcp:recv(Socket, 256, 1000)"),
        "epmd protocol send/receive paths should lower through Tcp intrinsics:\n{}",
        output
    );
    assert!(
        output.contains("gen_tcp:close(Socket)") && output.contains("erlang:port_close(Daemon)"),
        "epmd shutdown cleanup should close both socket and daemon port:\n{}",
        output
    );
    assert!(
        !output.contains("std_beam_tcp:")
            && !output.contains("std_beam_port:open")
            && !output.contains("std_beam_port:write")
            && !output.contains("std_beam_port:read")
            && !output.contains("std_beam_port:close"),
        "epmd smoke fixture must lower through compiler-owned BEAM intrinsics:\n{}",
        output
    );
}
