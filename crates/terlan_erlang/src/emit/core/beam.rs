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
use super::{erl_result_ok, exact_array_args};

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
