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
mod tests {
    use super::*;

    /// Asserts that an emitter source file does not contain a BEAM process
    /// protocol fragment that belongs in this module.
    ///
    /// Inputs:
    /// - `source_name`: human-readable Rust source label for assertion output.
    /// - `source`: Rust source text to inspect.
    /// - `fragment`: process protocol fragment that must stay centralized.
    ///
    /// Output:
    /// - Test assertion success when the fragment is absent.
    ///
    /// Transformation:
    /// - Performs a direct string containment check over checked-in backend
    ///   source so future process abstractions are forced to reuse this module
    ///   for spawn and request/reply protocol generation.
    fn assert_process_fragment_absent(source_name: &str, source: &str, fragment: &str) {
        assert!(
            !source.contains(fragment),
            "{source_name} must reuse emit::beam_process instead of embedding `{fragment}`"
        );
    }

    /// Verifies state-process start generation remains a shared BEAM loop shape.
    ///
    /// Inputs:
    /// - A simple initial integer state and one receive clause.
    ///
    /// Output:
    /// - Test passes when the generated Erlang expression contains the stable
    ///   loop, spawn, and `{ok, Pid}` fragments.
    ///
    /// Transformation:
    /// - Exercises the reusable helper without depending on a specific stdlib
    ///   abstraction such as Agent or future GenServer wrappers.
    #[test]
    fn state_process_start_emits_shared_loop_shape() {
        let expr = state_process_start(&ErlExpr::Int(1), "            stop ->\n                ok");
        let rendered = expr.render();

        assert!(rendered.contains("Loop = fun Loop(State) ->"));
        assert!(rendered.contains("receive"));
        assert!(rendered.contains("spawn(fun() -> Loop(1) end)"));
        assert!(rendered.contains("{ok, Pid}"));
    }

    /// Verifies result-backed process start generation stays centralized.
    ///
    /// Inputs:
    /// - A setup prelude, ok/error result patterns, and one receive clause.
    ///
    /// Output:
    /// - Test passes when the generated expression binds setup state, branches
    ///   on the setup result, spawns the shared loop, and preserves errors.
    ///
    /// Transformation:
    /// - Pins the helper used by callback-backed process abstractions whose
    ///   initial state is produced by a source-level initialization callback.
    #[test]
    fn state_process_start_from_result_emits_shared_setup_loop_shape() {
        let expr = state_process_start_from_result(
            "Server = make_server()",
            "init(Server)",
            "{ok, InitialState}",
            "InitialState",
            "{error, Error}",
            "{error, Error}",
            "                    stop ->\n                        ok",
        );
        let rendered = expr.render();

        assert!(rendered.contains("Server = make_server()"));
        assert!(rendered.contains("case init(Server) of"));
        assert!(rendered.contains("{ok, InitialState} ->"));
        assert!(rendered.contains("spawn(fun() -> Loop(InitialState) end)"));
        assert!(rendered.contains("{error, Error} ->"));
    }

    /// Verifies synchronous requests use generated reference matching.
    ///
    /// Inputs:
    /// - A process variable, request message, reply pattern, and reply body.
    ///
    /// Output:
    /// - Test passes when the generated Erlang expression uses `make_ref()`,
    ///   sends the request, and matches the supplied reply shape.
    ///
    /// Transformation:
    /// - Pins the reusable request/reply skeleton used by Agent and future
    ///   BEAM process-backed stdlib abstractions.
    #[test]
    fn sync_request_emits_reference_tagged_call_shape() {
        let expr = sync_request(
            &ErlExpr::Var("Pid".to_string()),
            "{get, self(), Ref}",
            "{Ref, Value}",
            "Value",
        );
        let rendered = expr.render();

        assert!(rendered.contains("Ref = make_ref()"));
        assert!(rendered.contains("Pid ! {get, self(), Ref}"));
        assert!(rendered.contains("{Ref, Value} -> Value"));
    }

    /// Verifies asynchronous messages return the original process handle.
    ///
    /// Inputs:
    /// - A process variable and fire-and-forget message.
    ///
    /// Output:
    /// - Test passes when the generated Erlang expression sends the message and
    ///   returns the same process expression.
    ///
    /// Transformation:
    /// - Pins the shared mutable-receiver-compatible send helper for process
    ///   abstractions that expose cast or stop operations.
    #[test]
    fn send_and_return_process_preserves_handle() {
        let expr = send_and_return_process(&ErlExpr::Var("Pid".to_string()), "stop");
        let rendered = expr.render();

        assert!(rendered.contains("Pid ! stop"));
        assert!(rendered.contains("Pid\nend)()"));
    }

    /// Verifies process protocol skeletons remain centralized.
    ///
    /// Inputs:
    /// - Checked-in `core.rs` and `syntax.rs` emitter sources.
    ///
    /// Output:
    /// - Test passes when direct spawn-loop and reference-request skeletons are
    ///   absent from the broader backend files.
    ///
    /// Transformation:
    /// - Treats this helper as the only owner for shared BEAM process emission
    ///   scaffolding while still allowing abstraction-specific receive clauses
    ///   to be supplied by caller-specific lowering code.
    #[test]
    fn shared_process_protocol_fragments_do_not_reappear_in_emitters() {
        let core = include_str!("core.rs");
        let syntax = include_str!("syntax.rs");

        for (source_name, source) in [("core.rs", core), ("syntax.rs", syntax)] {
            assert_process_fragment_absent(source_name, source, "spawn(fun() -> Loop(");
            assert_process_fragment_absent(source_name, source, "Ref = make_ref()");
            assert_process_fragment_absent(source_name, source, "receive\n        {Ref");
        }
    }
}
