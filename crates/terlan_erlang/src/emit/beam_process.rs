use super::erl::ErlExpr;

/// Builds a BEAM state-process loop expression.
///
/// Inputs:
/// - `initial_state`: lowered Erlang expression used as the first loop state.
/// - `receive_clauses`: Erlang receive clauses that recursively call `Loop`
///   when the process should continue.
///
/// Output:
/// - Erlang expression that spawns the loop and returns `{ok, Pid}`.
///
/// Transformation:
/// - Wraps backend-owned process state in a generated anonymous Erlang
///   function so stdlib process abstractions can share one spawn-loop shape.
pub(super) fn state_process_start(initial_state: &ErlExpr, receive_clauses: &str) -> ErlExpr {
    ErlExpr::Raw(format!(
        "(fun() ->\n    Loop = fun Loop(State) ->\n        receive\n{receive_clauses}\n        end\n    end,\n    Pid = spawn(fun() -> Loop({}) end),\n    {{ok, Pid}}\nend)()",
        initial_state.render()
    ))
}

/// Builds a BEAM state-process loop from a result-producing setup expression.
///
/// Inputs:
/// - `prelude`: Erlang statements evaluated before the setup result, usually
///   local bindings needed by receive clauses.
/// - `result_expr`: Erlang expression expected to return an ok/error result.
/// - `ok_pattern`: pattern that matches a successful setup result.
/// - `initial_state`: Erlang expression used as the loop's first state after
///   `ok_pattern` matches.
/// - `error_pattern`: pattern that matches a failed setup result.
/// - `error_body`: Erlang expression returned when `error_pattern` matches.
/// - `receive_clauses`: receive clauses that recursively call `Loop` when the
///   process should continue.
///
/// Output:
/// - Erlang expression that runs setup, spawns the loop on success, and returns
///   the failed setup result on error.
///
/// Transformation:
/// - Centralizes the setup/result variant of backend-owned process spawning so
///   callback-backed abstractions such as GenServer do not embed their own
///   spawn-loop protocol in broader emitter modules.
pub(super) fn state_process_start_from_result(
    prelude: &str,
    result_expr: &str,
    ok_pattern: &str,
    initial_state: &str,
    error_pattern: &str,
    error_body: &str,
    receive_clauses: &str,
) -> ErlExpr {
    ErlExpr::Raw(format!(
        "(fun() ->\n    {prelude},\n    case {result_expr} of\n        {ok_pattern} ->\n            Loop = fun Loop(State) ->\n                receive\n{receive_clauses}\n                end\n            end,\n            Pid = spawn(fun() -> Loop({initial_state}) end),\n            {{ok, Pid}};\n        {error_pattern} ->\n            {error_body}\n    end\nend)()",
    ))
}

/// Builds a reference-tagged synchronous process request expression.
///
/// Inputs:
/// - `process`: lowered Erlang expression that evaluates to the process handle.
/// - `message`: Erlang message expression that may reference the generated
///   `Ref` variable.
/// - `reply_pattern`: Erlang receive pattern that should include `Ref`.
/// - `reply_body`: Erlang body returned for the matching reply.
///
/// Output:
/// - Erlang expression that sends the message and returns the matching reply
///   body.
///
/// Transformation:
/// - Generates `make_ref()`, sends a private request, and waits only for the
///   matching tagged reply. The helper keeps BEAM message syntax inside the
///   backend rather than in Terlan source.
pub(super) fn sync_request(
    process: &ErlExpr,
    message: &str,
    reply_pattern: &str,
    reply_body: &str,
) -> ErlExpr {
    ErlExpr::Raw(format!(
        "(fun() ->\n    Ref = make_ref(),\n    {} ! {message},\n    receive\n        {reply_pattern} -> {reply_body}\n    end\nend)()",
        process.render()
    ))
}

/// Builds an asynchronous process message expression that returns the handle.
///
/// Inputs:
/// - `process`: lowered Erlang expression that evaluates to the process handle.
/// - `message`: Erlang message expression to send.
///
/// Output:
/// - Erlang expression that sends the message and returns the same process
///   handle.
///
/// Transformation:
/// - Emits fire-and-forget BEAM message delivery while preserving Terlan's
///   mutable receiver convention, where process-backed receivers remain stable
///   handles after mutation-like operations.
pub(super) fn send_and_return_process(process: &ErlExpr, message: &str) -> ErlExpr {
    let rendered_process = process.render();
    ErlExpr::Raw(format!(
        "(fun() ->\n    {rendered_process} ! {message},\n    {rendered_process}\nend)()",
    ))
}

#[cfg(test)]
#[path = "beam_process_test.rs"]
mod beam_process_test;
