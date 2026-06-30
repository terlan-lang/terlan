//! BEAM process primitive lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - Already-lowered Erlang argument expressions for BEAM-specific primitive
//!   intrinsics.
//!
//! Outputs:
//! - Erlang expressions that create, call, cast, stop, or otherwise interact
//!   with compiler-owned BEAM process abstractions.
//!
//! Transformations:
//! - Centralizes backend mailbox/protocol mechanics behind typed Terlan
//!   `std.beam` APIs while preserving stable source-level receiver and result
//!   shapes.

use super::super::beam_process;
use super::super::erl::ErlExpr;
use super::super::util::map_struct_name;
use super::{erl_result_ok, exact_array_args};

/// Builds a standard `Result.Err(Error)` expression from an existing reason variable.
///
/// Inputs:
/// - `code`: stable error code atom text.
/// - `prefix`: human-readable message prefix.
/// - `reason_expr`: Erlang expression or variable containing backend reason
///   details.
///
/// Output:
/// - Erlang source text for `{error, #error{...}}`.
///
/// Transformation:
/// - Converts target-specific reasons into the shared `std.core.Error.Error`
///   record while preserving the backend reason in the message field.
fn beam_error_result(code: &str, prefix: &str, reason_expr: &str) -> String {
    format!(
        "{{error, #{record}{{code = {code}, message = unicode:characters_to_binary(io_lib:format(\"{prefix}: ~p\", [{reason_expr}]))}}}}",
        record = map_struct_name("Error"),
        code = code,
        prefix = prefix,
        reason_expr = reason_expr
    )
}

/// Builds a standard `Result.Err(Error)` expression with a literal message.
///
/// Inputs:
/// - `code`: stable error code atom text.
/// - `message`: human-readable binary message text.
///
/// Output:
/// - Erlang source text for `{error, #error{...}}`.
///
/// Transformation:
/// - Emits a stable base error without depending on a target exception reason.
fn beam_error_literal(code: &str, message: &str) -> String {
    format!(
        "{{error, #{record}{{code = {code}, message = <<\"{message}\">>}}}}",
        record = map_struct_name("Error"),
        code = code,
        message = message
    )
}

/// Lowers `beam.agent.start` to a backend-owned BEAM process loop.
///
/// Inputs:
/// - `args`: one lowered initial state expression.
///
/// Output:
/// - `Ok(Pid)` where `Pid` is the opaque Agent handle.
///
/// Transformation:
/// - Creates a recursive Erlang anonymous function that owns state inside a
///   spawned process. The loop supports synchronous `get`, synchronous
///   `get_and_update`, synchronous `update`, asynchronous `cast`, and `stop`
///   messages without exposing BEAM message syntax in Terlan source.
pub(super) fn lower_beam_agent_start(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [initial] = exact_array_args(args)?;
    Some(beam_process::state_process_start(
        &initial,
        "            {get, From, Ref} ->\n                From ! {Ref, State},\n                Loop(State);\n            {get_and_update, Writer, From, Ref} ->\n                Pair = Writer(State),\n                NewState = element(1, Pair),\n                Value = element(2, Pair),\n                From ! {Ref, Value},\n                Loop(NewState);\n            {update, Writer, From, Ref} ->\n                NewState = Writer(State),\n                From ! {Ref, ok},\n                Loop(NewState);\n            {cast, Writer} ->\n                NewState = Writer(State),\n                Loop(NewState);\n            stop ->\n                ok",
    ))
}

/// Lowers `beam.agent.get` to a synchronous BEAM state read.
///
/// Inputs:
/// - `args`: one lowered Agent handle expression.
///
/// Output:
/// - Current state value returned by the BEAM-owned Agent process.
///
/// Transformation:
/// - Sends a private reference-tagged read request and waits for the matching
///   reply, preserving Agent opacity at the Terlan source boundary.
pub(super) fn lower_beam_agent_get(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [agent] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &agent,
        "{get, self(), Ref}",
        "{Ref, Value}",
        "Value",
    ))
}

/// Lowers `beam.agent.get_and_update` to a synchronous state transition and read.
///
/// Inputs:
/// - `args`: an Agent handle and writer function expression.
///
/// Output:
/// - The derived value returned by the writer's `{state, value}` pair.
///
/// Transformation:
/// - Sends a private reference-tagged request carrying the writer, updates the
///   BEAM-owned state from the pair's first element, and returns the second
///   element without exposing that backend tuple protocol in Terlan source.
pub(super) fn lower_beam_agent_get_and_update(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [agent, writer] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &agent,
        &format!("{{get_and_update, {}, self(), Ref}}", writer.render()),
        "{Ref, Value}",
        "Value",
    ))
}

/// Lowers `beam.agent.update` to a synchronous state transition.
///
/// Inputs:
/// - `args`: an Agent handle and writer function expression.
///
/// Output:
/// - The same Agent handle, allowing mutable receiver rebinding to remain
///   stable even though BEAM owns the actual process state.
///
/// Transformation:
/// - Sends a private reference-tagged update request, waits for acknowledgement,
///   and returns the stable handle as the updated receiver value.
pub(super) fn lower_beam_agent_update(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [agent, writer] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &agent,
        &format!("{{update, {}, self(), Ref}}", writer.render()),
        "{Ref, ok}",
        &agent.render(),
    ))
}

/// Lowers `beam.agent.cast` to an asynchronous state transition.
///
/// Inputs:
/// - `args`: an Agent handle and writer function expression.
///
/// Output:
/// - The same Agent handle, allowing mutable receiver rebinding to remain
///   stable while the state transition is delivered asynchronously.
///
/// Transformation:
/// - Sends a backend-private cast message carrying the writer function and
///   returns the stable Agent handle without waiting for acknowledgement.
pub(super) fn lower_beam_agent_cast(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [agent, writer] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(
        &agent,
        &format!("{{cast, {}}}", writer.render()),
    ))
}

/// Lowers `beam.agent.stop` to a BEAM process stop request.
///
/// Inputs:
/// - `args`: one Agent handle expression.
///
/// Output:
/// - The same Agent handle for mutable receiver rebinding compatibility.
///
/// Transformation:
/// - Sends the backend-private `stop` message and returns the stable handle.
pub(super) fn lower_beam_agent_stop(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [agent] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(&agent, "stop"))
}

/// Lowers `beam.gen_server.start` to a BEAM callback process loop.
///
/// Inputs:
/// - `args`: one lowered server implementation value.
///
/// Output:
/// - `Ok(Pid)` when `init(Server)` succeeds, otherwise the callback's
///   `Err(Error)` value.
///
/// Transformation:
/// - Starts a backend-owned state process whose messages dispatch to existing
///   receiver-first callback functions: `init(Server)`,
///   `handle_call(Server, State, Request)`, and
///   `handle_cast(Server, State, Event)`. This keeps GenServer behavior as
///   ordinary Terlan receiver methods while centralizing mailbox mechanics in
///   the BEAM backend.
pub(super) fn lower_beam_gen_server_start(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [server] = exact_array_args(args)?;
    Some(beam_process::state_process_start_from_result(
        &format!("Server = {}", server.render()),
        "init(Server)",
        "{ok, InitialState}",
        "InitialState",
        "{error, Error}",
        "{error, Error}",
        "                    {call, Request, From, Ref} ->\n                        case handle_call(Server, State, Request) of\n                            {ok, ReplyPair} ->\n                                NewState = element(1, ReplyPair),\n                                Reply = element(2, ReplyPair),\n                                From ! {Ref, {ok, Reply}},\n                                Loop(NewState);\n                            {error, Error} ->\n                                From ! {Ref, {error, Error}},\n                                Loop(State)\n                        end;\n                    {cast, Event} ->\n                        case handle_cast(Server, State, Event) of\n                            {ok, NewState} ->\n                                Loop(NewState);\n                            {error, _Error} ->\n                                Loop(State)\n                        end;\n                    stop ->\n                        ok",
    ))
}

/// Lowers `beam.gen_server.call` to a synchronous callback request.
///
/// Inputs:
/// - `args`: a GenServer handle and request value.
///
/// Output:
/// - The callback's `Result[Reply, Error]`.
///
/// Transformation:
/// - Sends a private reference-tagged call message and returns the matching
///   callback result without exposing BEAM message tuples to Terlan source.
pub(super) fn lower_beam_gen_server_call(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [server_ref, request] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &server_ref,
        &format!("{{call, {}, self(), Ref}}", request.render()),
        "{Ref, Result}",
        "Result",
    ))
}

/// Lowers `beam.gen_server.cast` to an asynchronous callback event.
///
/// Inputs:
/// - `args`: a GenServer handle and event value.
///
/// Output:
/// - The same GenServer handle for mutable receiver rebinding compatibility.
///
/// Transformation:
/// - Sends a backend-private cast message carrying the event and returns the
///   stable handle without waiting for acknowledgement.
pub(super) fn lower_beam_gen_server_cast(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [server_ref, event] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(
        &server_ref,
        &format!("{{cast, {}}}", event.render()),
    ))
}

/// Lowers `beam.gen_server.stop` to a BEAM process stop request.
///
/// Inputs:
/// - `args`: one GenServer handle expression.
///
/// Output:
/// - The same GenServer handle for mutable receiver rebinding compatibility.
///
/// Transformation:
/// - Sends the backend-private `stop` message and returns the stable handle.
pub(super) fn lower_beam_gen_server_stop(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [server_ref] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(&server_ref, "stop"))
}

/// Lowers `beam.native_bridge.start` to a shared BEAM bridge process proof.
///
/// Inputs:
/// - `args`: one native resource descriptor expression.
///
/// Output:
/// - `Ok(Pid)` where `Pid` is the opaque NativeBridge handle.
///
/// Transformation:
/// - Starts a backend-owned process through the shared BEAM process helper.
///   The current proof loop owns the resource descriptor and returns stable
///   not-loaded replies until real SafeNative transport attachment exists.
pub(super) fn lower_beam_native_bridge_start(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [resource] = exact_array_args(args)?;
    Some(beam_process::state_process_start(
        &resource,
        "            {call, _Command, From, Ref} ->\n                From ! {Ref, {error, {native_bridge_not_loaded, \"native bridge runtime not loaded\"}}},\n                Loop(State);\n            dispose ->\n                Loop(disposed);\n            stop ->\n                ok",
    ))
}

/// Lowers `beam.native_bridge.call` to the stable not-loaded bridge result.
///
/// Inputs:
/// - `args`: one bridge handle and one command value.
///
/// Output:
/// - The bridge process reply, currently the stable not-loaded result.
///
/// Transformation:
/// - Sends a reference-tagged call request through the shared BEAM process
///   helper. The current proof loop answers with a stable not-loaded result
///   without pretending that SafeNative transport is attached.
pub(super) fn lower_beam_native_bridge_call(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [bridge, command] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &bridge,
        &format!("{{call, {}, self(), Ref}}", command.render()),
        "{Ref, Result}",
        "Result",
    ))
}

/// Lowers `beam.native_bridge.dispose` to a stable mutable receiver value.
///
/// Inputs:
/// - `args`: one bridge handle expression.
///
/// Output:
/// - The bridge handle, preserving mutable receiver rebinding semantics.
///
/// Transformation:
/// - Sends the backend-private dispose message through the shared BEAM process
///   helper and preserves the stable receiver handle while real resource
///   disposal remains owned by SafeNative transport.
pub(super) fn lower_beam_native_bridge_dispose(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [bridge] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(&bridge, "dispose"))
}

/// Lowers `beam.native_bridge.stop` to a stable mutable receiver value.
///
/// Inputs:
/// - `args`: one bridge handle expression.
///
/// Output:
/// - The bridge handle, preserving mutable receiver rebinding semantics.
///
/// Transformation:
/// - Sends the backend-private stop message through the shared BEAM process
///   helper while final shutdown policy remains owned by the later
///   BEAM/SafeNative runtime.
pub(super) fn lower_beam_native_bridge_stop(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [bridge] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(&bridge, "stop"))
}

/// Lowers `beam.bytes.from_list` to an Erlang binary conversion.
///
/// Inputs:
/// - `args`: one list of integer byte values.
///
/// Output:
/// - Erlang binary built from the byte list.
///
/// Transformation:
/// - Uses `erlang:list_to_binary/1` as the BEAM-owned representation for
///   `std.beam.Bytes` so binary protocol tests do not construct raw target
///   syntax in Terlan source.
pub(super) fn lower_beam_bytes_from_list(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [values] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "erlang:list_to_binary({})",
        values.render()
    )))
}

/// Lowers `beam.bytes.to_list` to an Erlang byte-list conversion.
///
/// Inputs:
/// - `args`: one BEAM binary value.
///
/// Output:
/// - List of integer byte values.
///
/// Transformation:
/// - Uses `erlang:binary_to_list/1` to expose bytes through the typed Terlan
///   list contract.
pub(super) fn lower_beam_bytes_to_list(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [bytes] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "erlang:binary_to_list({})",
        bytes.render()
    )))
}

/// Lowers `beam.bytes.length` to BEAM byte size.
///
/// Inputs:
/// - `args`: one BEAM binary value.
///
/// Output:
/// - Integer byte length.
///
/// Transformation:
/// - Uses `erlang:byte_size/1`, preserving byte-count semantics for protocol
///   frames instead of text grapheme length.
pub(super) fn lower_beam_bytes_length(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [bytes] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "erlang:byte_size({})",
        bytes.render()
    )))
}

/// Lowers `beam.bytes.concat` to BEAM binary concatenation.
///
/// Inputs:
/// - `args`: left and right BEAM binary values.
///
/// Output:
/// - Concatenated binary.
///
/// Transformation:
/// - Emits one binary construction expression so protocol frame composition
///   remains target-owned and allocation behavior is explicit.
pub(super) fn lower_beam_bytes_concat(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "<<({})/binary, ({})/binary>>",
        left.render(),
        right.render()
    )))
}

/// Lowers `beam.timeout.milliseconds` to a BEAM receive timeout value.
///
/// Inputs:
/// - `args`: one integer millisecond value.
///
/// Output:
/// - Integer timeout expression.
///
/// Transformation:
/// - Preserves the integer expression because BEAM APIs use millisecond
///   integers directly for socket and receive timeouts.
pub(super) fn lower_beam_timeout_milliseconds(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [milliseconds] = exact_array_args(args)?;
    Some(milliseconds)
}

/// Lowers `beam.timeout.forever` to the BEAM infinity timeout atom.
///
/// Inputs:
/// - `args`: no arguments.
///
/// Output:
/// - Erlang `infinity`.
///
/// Transformation:
/// - Keeps the target's unbounded timeout sentinel behind the typed Terlan
///   `Timeout` constructor.
pub(super) fn lower_beam_timeout_forever(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [] = exact_array_args(args)?;
    Some(ErlExpr::Raw("infinity".to_string()))
}

/// Lowers `beam.tcp.connect` to `gen_tcp:connect/4`.
///
/// Inputs:
/// - `args`: host binary, port integer, and BEAM timeout expression.
///
/// Output:
/// - `Result[TcpSocket, Error]`.
///
/// Transformation:
/// - Opens a passive binary TCP socket through OTP and maps backend failures
///   into the standard `Error` record shape.
pub(super) fn lower_beam_tcp_connect(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [host, port, timeout] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "case gen_tcp:connect(erlang:binary_to_list({}), {}, [binary, {{packet, 0}}, {{active, false}}], {}) of\n    {{ok, Socket}} -> {{ok, Socket}};\n    {{error, Reason}} -> {}\nend",
        host.render(),
        port.render(),
        timeout.render(),
        beam_error_result("tcp_connect_failed", "tcp connect failed", "Reason")
    )))
}

/// Lowers `beam.tcp.send` to `gen_tcp:send/2`.
///
/// Inputs:
/// - `args`: TCP socket handle and binary payload.
///
/// Output:
/// - `Result[Unit, Error]`.
///
/// Transformation:
/// - Sends the binary frame and normalizes BEAM send errors into the standard
///   `Error` record shape.
pub(super) fn lower_beam_tcp_send(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [socket, bytes] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "case gen_tcp:send({}, {}) of\n    ok -> {{ok, unit}};\n    {{error, Reason}} -> {}\nend",
        socket.render(),
        bytes.render(),
        beam_error_result("tcp_send_failed", "tcp send failed", "Reason")
    )))
}

/// Lowers `beam.tcp.receive` to `gen_tcp:recv/3`.
///
/// Inputs:
/// - `args`: TCP socket handle, maximum byte count, and timeout expression.
///
/// Output:
/// - `Result[Bytes, Error]`.
///
/// Transformation:
/// - Receives a passive socket frame and preserves the returned binary as the
///   `Bytes` value expected by Terlan protocol tests.
pub(super) fn lower_beam_tcp_receive(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [socket, max_bytes, timeout] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "case gen_tcp:recv({}, {}, {}) of\n    {{ok, Bytes}} -> {{ok, Bytes}};\n    {{error, Reason}} -> {}\nend",
        socket.render(),
        max_bytes.render(),
        timeout.render(),
        beam_error_result("tcp_receive_failed", "tcp receive failed", "Reason")
    )))
}

/// Lowers `beam.tcp.close` to `gen_tcp:close/1`.
///
/// Inputs:
/// - `args`: one TCP socket handle.
///
/// Output:
/// - Terlan `Unit`.
///
/// Transformation:
/// - Closes the target-owned socket and normalizes the public return to Unit.
pub(super) fn lower_beam_tcp_close(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [socket] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "begin gen_tcp:close({}), unit end",
        socket.render()
    )))
}

/// Lowers `beam.port.open` to `erlang:open_port/2`.
///
/// Inputs:
/// - `args`: one `std.beam.Port.Command` record expression.
///
/// Output:
/// - `Result[Port, Error]`.
///
/// Transformation:
/// - Reads the ordinary Terlan command record, starts a binary stdio port, and
///   maps startup exceptions into the standard `Error` record shape.
pub(super) fn lower_beam_port_open(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [command] = exact_array_args(args)?;
    let command_record = map_struct_name("Command");
    let env_record = map_struct_name("EnvVar");
    Some(ErlExpr::Raw(format!(
        "case {} of\n    #{}{{executable = Executable, arguments = Arguments, environment = Environment}} ->\n        try\n            Port = erlang:open_port({{spawn_executable, erlang:binary_to_list(Executable)}}, [binary, exit_status, use_stdio, stderr_to_stdout, stream, {{args, [erlang:binary_to_list(Arg) || Arg <- Arguments]}}, {{env, [{{erlang:binary_to_list(Key), erlang:binary_to_list(Value)}} || #{}{{key = Key, value = Value}} <- Environment]}}]),\n            {{ok, Port}}\n        catch\n            _Class:Reason -> {}\n        end;\n    _ -> {}\nend",
        command.render(),
        command_record,
        env_record,
        beam_error_result("port_open_failed", "port open failed", "Reason"),
        beam_error_literal("invalid_command", "invalid command")
    )))
}

/// Lowers `beam.port.write` to a BEAM port command.
///
/// Inputs:
/// - `args`: port handle and binary payload.
///
/// Output:
/// - `Result[Unit, Error]`.
///
/// Transformation:
/// - Sends a command frame to the port owner protocol and maps invalid port
///   failures into the standard `Error` record shape.
pub(super) fn lower_beam_port_write(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [port, bytes] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "try\n    {} ! {{self(), {{command, {}}}}},\n    {{ok, unit}}\ncatch\n    _Class:Reason -> {}\nend",
        port.render(),
        bytes.render(),
        beam_error_result("port_write_failed", "port write failed", "Reason")
    )))
}

/// Lowers `beam.port.read` to a BEAM receive over port messages.
///
/// Inputs:
/// - `args`: port handle, maximum byte count, and timeout expression.
///
/// Output:
/// - `Result[Bytes, Error]`.
///
/// Transformation:
/// - Waits for data or exit-status messages from the port and returns at most
///   the requested number of bytes.
pub(super) fn lower_beam_port_read(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [port, max_bytes, timeout] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "receive\n    {{{}, {{data, Data}}}} -> {{ok, binary:part(Data, 0, erlang:min(erlang:byte_size(Data), {}))}};\n    {{{}, {{exit_status, Status}}}} -> {}\nafter {} -> {}\nend",
        port.render(),
        max_bytes.render(),
        port.render(),
        beam_error_result("port_exited", "port exited", "Status"),
        timeout.render(),
        beam_error_literal("timeout", "port read timed out")
    )))
}

/// Lowers `beam.port.close` to `erlang:port_close/1`.
///
/// Inputs:
/// - `args`: one BEAM port handle.
///
/// Output:
/// - Terlan `Unit`.
///
/// Transformation:
/// - Closes the target-owned external process handle and normalizes the public
///   return to Unit.
pub(super) fn lower_beam_port_close(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [port] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "begin catch erlang:port_close({}), unit end",
        port.render()
    )))
}

/// Lowers `beam.supervisor.child_spec` to a backend-private child spec tuple.
///
/// Inputs:
/// - `args`: one lowered child value expression.
///
/// Output:
/// - A private `{terlan_child_spec, Child}` tuple used by the local Supervisor
///   proof.
///
/// Transformation:
/// - Preserves the supervised value behind the opaque `ChildSpec[P]` source
///   contract without exposing BEAM supervision tuple formats to Terlan code.
pub(super) fn lower_beam_supervisor_child_spec(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Raw(format!(
        "{{terlan_child_spec, {}}}",
        value.render()
    )))
}

/// Lowers `beam.supervisor.start_root` to a supervisor process loop.
///
/// Inputs:
/// - `args`: no arguments.
///
/// Output:
/// - `Ok(Pid)` where `Pid` is the opaque Supervisor handle.
///
/// Transformation:
/// - Starts a backend-owned process that accepts the same private
///   `start_child` and `stop_child` messages used by receiver-method
///   Supervisor operations. The current proof supervisor returns the child
///   value as the started result while restart strategy remains a later
///   runtime-owned capability.
pub(super) fn lower_beam_supervisor_start_root(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [] = exact_array_args(args)?;
    Some(beam_process::state_process_start(
        &ErlExpr::Atom("running".to_string()),
        "            {start_child, Child, From, Ref} ->\n                From ! {Ref, {ok, Child}},\n                Loop(State);\n            {stop_child, _Value} ->\n                Loop(State);\n            stop ->\n                ok",
    ))
}

/// Lowers `beam.supervisor.start` to the current Supervisor process protocol.
///
/// Inputs:
/// - `args`: a Supervisor handle and one lowered child specification.
///
/// Output:
/// - A reference-tagged `Result[P, Error]` reply from the Supervisor process,
///   or an invalid-child-spec error for malformed local proof values.
///
/// Transformation:
/// - Validates the child spec shape, then delegates the start request through
///   the shared BEAM process helper. Real restart strategy, child ownership,
///   and failure handling remain owned by the later Supervisor runtime module.
pub(super) fn lower_beam_supervisor_start(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [supervisor, spec] = exact_array_args(args)?;
    let request = beam_process::sync_request(
        &supervisor,
        "{start_child, Child, self(), Ref}",
        "{Ref, Result}",
        "Result",
    );
    Some(ErlExpr::Raw(format!(
        "case {} of\n    {{terlan_child_spec, Child}} -> {};\n    _ -> {{error, {{invalid_child_spec, \"invalid child spec\"}}}}\nend",
        spec.render(),
        request.render()
    )))
}

/// Lowers `beam.supervisor.stop` to a stable mutable receiver value.
///
/// Inputs:
/// - `args`: a Supervisor handle and one supervised value expression.
///
/// Output:
/// - The Supervisor handle after a fire-and-forget stop-child message, allowing
///   mutable receiver rebinding to stay consistent with other compiler-owned
///   mutable receiver intrinsics.
///
/// Transformation:
/// - Sends the stop request through the shared BEAM process helper while real
///   child shutdown semantics remain owned by a later Supervisor runtime
///   module.
pub(super) fn lower_beam_supervisor_stop(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [supervisor, value] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(
        &supervisor,
        &format!("{{stop_child, {}}}", value.render()),
    ))
}

/// Lowers `beam.task.start` to a backend-owned BEAM work process.
///
/// Inputs:
/// - `args`: one lowered zero-argument work function.
///
/// Output:
/// - `Ok(Pid)` where `Pid` is the opaque BEAM Task handle.
///
/// Transformation:
/// - Spawns a state process whose initial state is `Ok(work())`. The process
///   supports synchronous result reads and cancellation through the shared BEAM
///   process helper, keeping task mailbox details out of Terlan source.
pub(super) fn lower_beam_task_start(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [work] = exact_array_args(args)?;
    Some(beam_process::state_process_start(
        &erl_result_ok(ErlExpr::Raw(format!("({})()", work.render()))),
        "            {result, From, Ref} ->\n                From ! {Ref, State},\n                Loop(State);\n            cancel ->\n                ok",
    ))
}

/// Lowers `beam.task.result` to a synchronous BEAM Task result read.
///
/// Inputs:
/// - `args`: one lowered BEAM Task handle expression.
///
/// Output:
/// - The stored `Result[T, Error]` value for the task.
///
/// Transformation:
/// - Sends a private reference-tagged request and returns the matching stored
///   result without exposing BEAM monitor or mailbox protocol in source.
pub(super) fn lower_beam_task_result(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [task] = exact_array_args(args)?;
    Some(beam_process::sync_request(
        &task,
        "{result, self(), Ref}",
        "{Ref, Result}",
        "Result",
    ))
}

/// Lowers `beam.task.cancel` to a BEAM Task cancellation request.
///
/// Inputs:
/// - `args`: one lowered BEAM Task handle expression.
///
/// Output:
/// - The same task handle for mutable receiver rebinding compatibility.
///
/// Transformation:
/// - Sends the backend-private cancel message and returns the stable handle.
pub(super) fn lower_beam_task_cancel(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [task] = exact_array_args(args)?;
    Some(beam_process::send_and_return_process(&task, "cancel"))
}
