use std::collections::BTreeMap;

use super::test_support::{
    test_core_module_for_syntax, test_primitive_intrinsic_call, test_runtime_capability_call,
    test_string_intrinsic_call,
};
use super::{lower_core_expr_to_erlang, try_emit_core_module_to_erlang_with_syntax_bridge};
use terlan_syntax::parse_module_as_syntax_output;
use terlan_typeck::{
    CoreExpr, CorePattern, CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreTupleTypeElem,
    CoreType,
};

/// Verifies `core.string.contains` lowers Erlang sentinel search into booleans.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Builds a CoreIR intrinsic call and renders the private backend-lowered
///   Erlang expression to inspect the target semantics.
#[test]
fn core_string_contains_intrinsic_lowers_to_erlang_search_case() {
    let call = test_string_intrinsic_call(
        CorePrimitiveIntrinsic::StringContains,
        vec![
            CoreExpr::Binary("\"hello\"".to_string()),
            CoreExpr::Binary("\"ell\"".to_string()),
        ],
        CoreType::Bool,
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("string contains intrinsic should lower")
        .render();

    assert!(rendered.contains("string:find(\"hello\", \"ell\")"));
    assert!(rendered.contains("'nomatch'"));
    assert!(rendered.contains("false"));
    assert!(rendered.contains("true"));
}

/// Verifies CoreIR casts lower as identity expressions for Erlang output.
///
/// Inputs:
/// - A directly constructed assignment-compatible CoreIR cast.
///
/// Output:
/// - Test passes when the Erlang expression is the wrapped value.
///
/// Transformation:
/// - Lowers `CoreExpr::Cast` through the CoreIR Erlang emitter and confirms
///   the backend does not introduce target-specific coercion for a cast that
///   typechecking has already accepted.
#[test]
fn core_cast_expr_lowers_to_erlang_wrapped_expr() {
    let expr = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Int(42)),
        target_type: CoreType::Int,
    };
    let rendered = lower_core_expr_to_erlang(&expr)
        .expect("assignment-compatible cast should lower")
        .render();

    assert_eq!(rendered, "42");
}

/// Verifies `runtime.console.println` lowers through backend-owned BEAM IO.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Builds a CoreIR runtime capability call and verifies the backend emits
///   `io:format/2` behind the portable `std.io.Console.println` surface
///   while normalizing the returned source value to `unit`.
#[test]
fn runtime_console_println_capability_lowers_to_erlang_io_format() {
    let call = test_runtime_capability_call(
        CoreRuntimeCapability::ConsolePrintln,
        vec![CoreExpr::Binary("\"hello\"".to_string())],
        CoreType::Named("Unit".to_string()),
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("console println runtime capability should lower")
        .render();

    assert_eq!(
        rendered,
        "begin io:format(\"~ts~n\", [\"hello\"]), unit end"
    );
}

/// Verifies BEAM byte-buffer intrinsics lower to Erlang binary operations.
///
/// Inputs:
/// - CoreIR primitive calls for byte list conversion, byte-list extraction,
///   byte length, and binary concatenation.
///
/// Output:
/// - Test assertions over rendered Erlang expressions.
///
/// Transformation:
/// - Confirms `std.beam.Bytes` operations use BEAM binaries directly while
///   retaining the typed source API boundary.
#[test]
fn beam_bytes_intrinsics_lower_to_erlang_binary_operations() {
    let from_list = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamBytesFromList,
        vec![CoreExpr::Var("values".to_string())],
        CoreType::Named("Bytes".to_string()),
    );
    let to_list = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamBytesToList,
        vec![CoreExpr::Var("bytes".to_string())],
        CoreType::List(Box::new(CoreType::Int)),
    );
    let length = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamBytesLength,
        vec![CoreExpr::Var("bytes".to_string())],
        CoreType::Int,
    );
    let concat = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamBytesConcat,
        vec![
            CoreExpr::Var("left".to_string()),
            CoreExpr::Var("right".to_string()),
        ],
        CoreType::Named("Bytes".to_string()),
    );

    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&from_list)
            .expect("bytes from_list should lower")
            .render(),
        "erlang:list_to_binary(Values)"
    );
    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&to_list)
            .expect("bytes to_list should lower")
            .render(),
        "erlang:binary_to_list(Bytes)"
    );
    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&length)
            .expect("bytes length should lower")
            .render(),
        "erlang:byte_size(Bytes)"
    );
    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&concat)
            .expect("bytes concat should lower")
            .render(),
        "<<(Left)/binary, (Right)/binary>>"
    );
}

/// Verifies BEAM timeout constructors lower to receive-compatible values.
///
/// Inputs:
/// - CoreIR primitive calls for finite and unbounded timeout constructors.
///
/// Output:
/// - Test assertions over rendered Erlang expressions.
///
/// Transformation:
/// - Confirms `std.beam.Timeout` hides BEAM's integer-or-infinity ABI behind
///   source-level constructors.
#[test]
fn beam_timeout_intrinsics_lower_to_erlang_timeout_values() {
    let milliseconds = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTimeoutMilliseconds,
        vec![CoreExpr::Int(250)],
        CoreType::Named("Timeout".to_string()),
    );
    let forever = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTimeoutForever,
        Vec::new(),
        CoreType::Named("Timeout".to_string()),
    );

    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&milliseconds)
            .expect("milliseconds should lower")
            .render(),
        "250"
    );
    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&forever)
            .expect("forever should lower")
            .render(),
        "infinity"
    );
}

/// Verifies BEAM TCP intrinsics lower to passive binary `gen_tcp` calls.
///
/// Inputs:
/// - CoreIR primitive calls for connect, send, receive, and close.
///
/// Output:
/// - Test assertions over rendered Erlang expressions.
///
/// Transformation:
/// - Exercises the socket lifecycle lowering needed by daemon parity tests and
///   confirms recoverable backend failures map into the standard `Error`
///   record shape.
#[test]
fn beam_tcp_intrinsics_lower_to_erlang_gen_tcp_calls() {
    let connect = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTcpConnect,
        vec![
            CoreExpr::Var("host".to_string()),
            CoreExpr::Var("port".to_string()),
            CoreExpr::Var("timeout".to_string()),
        ],
        CoreType::Named("Result".to_string()),
    );
    let send = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTcpSend,
        vec![
            CoreExpr::Var("socket".to_string()),
            CoreExpr::Var("bytes".to_string()),
        ],
        CoreType::Named("Result".to_string()),
    );
    let receive = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTcpReceive,
        vec![
            CoreExpr::Var("socket".to_string()),
            CoreExpr::Var("max_bytes".to_string()),
            CoreExpr::Var("timeout".to_string()),
        ],
        CoreType::Named("Result".to_string()),
    );
    let close = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamTcpClose,
        vec![CoreExpr::Var("socket".to_string())],
        CoreType::Named("Unit".to_string()),
    );

    let connect_rendered = super::lower_core_intrinsic_call_to_erlang(&connect)
        .expect("tcp connect should lower")
        .render();
    assert!(connect_rendered.contains("gen_tcp:connect(erlang:binary_to_list(Host), Port"));
    assert!(connect_rendered.contains("[binary, {packet, 0}, {active, false}]"));
    assert!(connect_rendered.contains("#error{code = tcp_connect_failed"));

    let send_rendered = super::lower_core_intrinsic_call_to_erlang(&send)
        .expect("tcp send should lower")
        .render();
    assert!(send_rendered.contains("gen_tcp:send(Socket, Bytes)"));
    assert!(send_rendered.contains("#error{code = tcp_send_failed"));

    let receive_rendered = super::lower_core_intrinsic_call_to_erlang(&receive)
        .expect("tcp receive should lower")
        .render();
    assert!(receive_rendered.contains("gen_tcp:recv(Socket, Max_bytes, Timeout)"));
    assert!(receive_rendered.contains("{ok, Bytes} -> {ok, Bytes}"));
    assert!(receive_rendered.contains("#error{code = tcp_receive_failed"));

    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&close)
            .expect("tcp close should lower")
            .render(),
        "begin gen_tcp:close(Socket), unit end"
    );
}

/// Verifies BEAM port intrinsics lower to external process port operations.
///
/// Inputs:
/// - CoreIR primitive calls for open, write, read, and close.
///
/// Output:
/// - Test assertions over rendered Erlang expressions.
///
/// Transformation:
/// - Exercises command-record decoding, binary stdio startup, port messaging,
///   receive timeouts, and cleanup lowering needed before epmd parity tests can
///   move from Erlang Common Test to Terlan.
#[test]
fn beam_port_intrinsics_lower_to_erlang_port_operations() {
    let open = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamPortOpen,
        vec![CoreExpr::Var("command".to_string())],
        CoreType::Named("Result".to_string()),
    );
    let write = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamPortWrite,
        vec![
            CoreExpr::Var("port".to_string()),
            CoreExpr::Var("bytes".to_string()),
        ],
        CoreType::Named("Result".to_string()),
    );
    let read = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamPortRead,
        vec![
            CoreExpr::Var("port".to_string()),
            CoreExpr::Var("max_bytes".to_string()),
            CoreExpr::Var("timeout".to_string()),
        ],
        CoreType::Named("Result".to_string()),
    );
    let close = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::BeamPortClose,
        vec![CoreExpr::Var("port".to_string())],
        CoreType::Named("Unit".to_string()),
    );

    let open_rendered = super::lower_core_intrinsic_call_to_erlang(&open)
        .expect("port open should lower")
        .render();
    assert!(open_rendered.contains("#command{executable = Executable"));
    assert!(open_rendered.contains("erlang:open_port({spawn_executable"));
    assert!(open_rendered.contains("#envvar{key = Key, value = Value}"));
    assert!(open_rendered.contains("#error{code = port_open_failed"));
    assert!(open_rendered.contains("#error{code = invalid_command"));

    let write_rendered = super::lower_core_intrinsic_call_to_erlang(&write)
        .expect("port write should lower")
        .render();
    assert!(write_rendered.contains("Port ! {self(), {command, Bytes}}"));
    assert!(write_rendered.contains("#error{code = port_write_failed"));

    let read_rendered = super::lower_core_intrinsic_call_to_erlang(&read)
        .expect("port read should lower")
        .render();
    assert!(read_rendered.contains("receive"));
    assert!(read_rendered.contains("{Port, {data, Data}} -> {ok, binary:part"));
    assert!(read_rendered.contains("{Port, {exit_status, Status}}"));
    assert!(read_rendered.contains("after Timeout"));
    assert!(read_rendered.contains("#error{code = timeout"));

    assert_eq!(
        super::lower_core_intrinsic_call_to_erlang(&close)
            .expect("port close should lower")
            .render(),
        "begin catch erlang:port_close(Port), unit end"
    );
}

/// Verifies BEAM primitive lowerings reject malformed arity.
///
/// Inputs:
/// - CoreIR primitive calls with intentionally missing arguments.
///
/// Output:
/// - Test passes when the backend declines to emit each malformed call.
///
/// Transformation:
/// - Exercises defensive arity checks for newly added BEAM primitives so a bad
///   CoreIR payload cannot produce syntactically invalid Erlang.
#[test]
fn beam_primitive_intrinsics_reject_malformed_arity() {
    for intrinsic in [
        CorePrimitiveIntrinsic::BeamBytesFromList,
        CorePrimitiveIntrinsic::BeamTimeoutMilliseconds,
        CorePrimitiveIntrinsic::BeamTcpConnect,
        CorePrimitiveIntrinsic::BeamPortOpen,
    ] {
        let call = test_primitive_intrinsic_call(intrinsic, Vec::new(), CoreType::Int);
        assert!(
            super::lower_core_intrinsic_call_to_erlang(&call).is_none(),
            "malformed BEAM intrinsic arity should not lower"
        );
    }
}

/// Verifies `core.type.type_of` lowers to a backend-private type value.
///
/// Inputs:
/// - A CoreIR primitive intrinsic call with one integer expression.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Classifies the CoreIR expression shape statically and emits an internal
///   type-value atom without using BEAM runtime reflection.
#[test]
fn core_type_of_intrinsic_lowers_to_erlang_type_atom() {
    let call = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::TypeOf,
        vec![CoreExpr::Int(1)],
        CoreType::Named("Type".to_string()),
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("type_of intrinsic should lower")
        .render();

    assert_eq!(rendered, "'terlan_type_int'");
}

/// Verifies `core.type.is_type` lowers to internal type-value comparison.
///
/// Inputs:
/// - A CoreIR primitive intrinsic call comparing an integer expression to the
///   implicit `Int` type value.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Lowers both sides to backend-private type atoms and compares them with
///   exact equality.
#[test]
fn core_is_type_intrinsic_lowers_to_erlang_type_atom_comparison() {
    let call = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::IsType,
        vec![CoreExpr::Int(1), CoreExpr::Var("Int".to_string())],
        CoreType::Bool,
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("is_type intrinsic should lower")
        .render();

    assert_eq!(rendered, "'terlan_type_int' =:= 'terlan_type_int'");
}

/// Verifies CoreIR list comprehensions lower to Erlang comprehensions.
///
/// Inputs:
/// - A backend-neutral CoreIR list-comprehension expression with a direct
///   variable generator pattern.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Lowers the CoreIR comprehension through the formal CoreIR backend path,
///   preserving generator binding, source expression, and yielded expression
///   as Erlang list-comprehension syntax.
#[test]
fn core_list_comprehension_lowers_to_erlang_list_comprehension() {
    let expr = CoreExpr::ListComprehension {
        expr: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Int(1)),
        }),
        pattern: CorePattern::Var("value".to_string()),
        source: Box::new(CoreExpr::Var("values".to_string())),
        guard: None,
    };

    let rendered = lower_core_expr_to_erlang(&expr)
        .expect("CoreIR list comprehension should lower")
        .render();

    assert_eq!(rendered, "[Value + 1 || Value <- Values]");
}

/// Verifies CoreIR list-comprehension lowering supports Erlang-native
/// destructuring patterns.
///
/// Inputs:
/// - A CoreIR list comprehension whose generator pattern destructures a tuple.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Recursively lowers the CoreIR tuple pattern into Erlang generator pattern
///   syntax so traversal lowering can rely on backend-native pattern matching
///   for supported pattern shapes.
#[test]
fn core_list_comprehension_lowers_destructuring_pattern() {
    let expr = CoreExpr::ListComprehension {
        expr: Box::new(CoreExpr::Var("left".to_string())),
        pattern: CorePattern::Tuple(vec![
            CorePattern::Var("left".to_string()),
            CorePattern::Wildcard,
        ]),
        source: Box::new(CoreExpr::Var("pairs".to_string())),
        guard: None,
    };

    let rendered = lower_core_expr_to_erlang(&expr)
        .expect("CoreIR destructuring list comprehension should lower")
        .render();

    assert_eq!(rendered, "[Left || {Left, _} <- Pairs]");
}

/// Verifies `core.list.iterator` lowers to the BEAM list state.
///
/// Inputs:
/// - A CoreIR primitive intrinsic call with one list expression.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Lowers the portable `List.iterator` intrinsic through the backend-owned
///   traversal contract and proves the current BEAM representation is reused
///   behind the opaque iterator API.
#[test]
fn core_list_iterator_intrinsic_lowers_to_erlang_list_state() {
    let call = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::ListIterator,
        vec![CoreExpr::List(vec![CoreExpr::Int(1), CoreExpr::Int(2)])],
        CoreType::List(Box::new(CoreType::Named("Dynamic".to_string()))),
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("list iterator intrinsic should lower")
        .render();

    assert_eq!(rendered, "[1, 2]");
}

/// Verifies `core.iterator.next` lowers to explicit state-passing traversal.
///
/// Inputs:
/// - A CoreIR primitive intrinsic call with one iterator-state expression.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Lowers the portable `Iterator.next` intrinsic into a backend case
///   expression that returns `Some({value, next})` for a non-empty list state
///   and `None` for the empty state.
#[test]
fn core_iterator_next_intrinsic_lowers_to_erlang_option_step() {
    let call = test_primitive_intrinsic_call(
        CorePrimitiveIntrinsic::IteratorNext,
        vec![CoreExpr::Var("iterator".to_string())],
        CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::Named("Dynamic".to_string())),
                CoreTupleTypeElem::Type(CoreType::List(Box::new(CoreType::Named(
                    "Dynamic".to_string(),
                )))),
            ])],
        },
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("iterator next intrinsic should lower")
        .render();

    assert!(rendered.contains("case Iterator of"));
    assert!(rendered.contains(
        "[_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}}"
    ));
    assert!(rendered.contains("[] -> 'none'"));
}
#[test]
fn core_string_byte_size_intrinsic_lowers_to_erlang_utf8_byte_size() {
    let call = test_string_intrinsic_call(
        CorePrimitiveIntrinsic::StringByteSize,
        vec![CoreExpr::Binary("\"hello\"".to_string())],
        CoreType::Int,
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("string byte_size intrinsic should lower")
        .render();

    assert_eq!(
        rendered,
        "erlang:byte_size(unicode:characters_to_binary(\"hello\"))"
    );
}

#[test]
fn core_string_concat_intrinsic_lowers_to_binary_safe_iolist_conversion() {
    let call = test_string_intrinsic_call(
        CorePrimitiveIntrinsic::StringConcat,
        vec![CoreExpr::List(vec![
            CoreExpr::Binary("\"id:\"".to_string()),
            CoreExpr::Var("UserId".to_string()),
        ])],
        CoreType::Binary,
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("string concat intrinsic should lower")
        .render();

    assert_eq!(rendered, "unicode:characters_to_list([\"id:\", UserId])");
}

/// Verifies `core.string.split_once` lowers to the backend option shape.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Builds a CoreIR intrinsic call and checks that Erlang split results are
///   converted to `some({Left, Right})` or `none`.
#[test]
fn core_string_split_once_intrinsic_lowers_to_option_shape() {
    let call = test_string_intrinsic_call(
        CorePrimitiveIntrinsic::StringSplitOnce,
        vec![
            CoreExpr::Binary("\"a,b\"".to_string()),
            CoreExpr::Binary("\",\"".to_string()),
        ],
        CoreType::Union(vec![
            CoreType::Tuple(vec![]),
            CoreType::AtomLiteral("none".to_string()),
        ]),
    );

    let rendered = super::lower_core_intrinsic_call_to_erlang(&call)
        .expect("string split_once intrinsic should lower")
        .render();

    assert!(rendered.contains("string:split(\"a,b\", \",\", 'leading')"));
    assert!(rendered.contains("{'some', {Left, Right}}"));
    assert!(rendered.contains("[_] -> 'none'"));
}

/// Verifies compiler intrinsic annotations replace source placeholder bodies.
///
/// Inputs:
/// - None; builds a small syntax-output module with an annotated string
///   intrinsic function.
///
/// Output:
/// - Test assertion over the generated Erlang source.
///
/// Transformation:

/// - A directly constructed `CoreExpr::FunctionCall` over a local function
///   value variable.
///
/// Output:
/// - Test passes when Erlang rendering uses expression application rather
///   than named local-function call syntax.
///
/// Transformation:
/// - Lowers backend-neutral callable-value CoreIR into the Erlang AST model
///   and renders the result for the selected conservative subset.
#[test]
fn core_function_call_lowers_to_erlang_apply() {
    let expr = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("f".to_string())),
        args: vec![CoreExpr::Var("value".to_string())],
    };

    let lowered = lower_core_expr_to_erlang(&expr).expect("lower function-value call");

    assert_eq!(lowered.render(), "(F)(Value)");
}

/// Verifies trait-backed indexed reads are not emitted as plain local calls.
///
/// Inputs:
/// - A CoreIR call to the reserved `IndexGet.get_at` lowering target.
///
/// Output:
/// - Test passes when the Core Erlang backend refuses the expression.
///
/// Transformation:
/// - Protects the N0.2 indexed-read path from silently rendering
///   `indexget_get_at(...)` before the backend has module-aware trait-wrapper
///   dispatch for `IndexGet` conformances.
#[test]
fn core_index_get_call_waits_for_trait_wrapper_backend_dispatch() {
    let expr = CoreExpr::Call {
        function: "IndexGet.get_at".to_string(),
        args: vec![CoreExpr::Var("values".to_string()), CoreExpr::Int(0)],
    };

    assert!(lower_core_expr_to_erlang(&expr).is_none());
}

/// Verifies trait-backed indexed writes are not emitted as plain local calls.
///
/// Inputs:
/// - A CoreIR call to the reserved `IndexSet.set_at` lowering target.
///
/// Output:
/// - Test passes when the Core Erlang backend refuses the expression.
///
/// Transformation:
/// - Protects the N0.3 indexed-assignment path from silently rendering
///   `indexset_set_at(...)` before the backend has module-aware trait-wrapper
///   dispatch and mutable receiver rebinding for `IndexSet` conformances.
#[test]
fn core_index_set_call_waits_for_trait_wrapper_backend_dispatch() {
    let expr = CoreExpr::Call {
        function: "IndexSet.set_at".to_string(),
        args: vec![
            CoreExpr::Var("values".to_string()),
            CoreExpr::Int(0),
            CoreExpr::Int(1),
        ],
    };

    assert!(lower_core_expr_to_erlang(&expr).is_none());
}

#[test]
fn core_module_syntax_bridge_emit_delegates_after_identity_validation() {
    let module = parse_module_as_syntax_output(
        r#"
module core_emit_gate.

pub value(): Int ->
1.
"#,
    )
    .expect("parse core emit gate fixture");
    let core = test_core_module_for_syntax(&module);

    let output = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("core syntax-bridge emit should succeed");

    assert!(output.contains("-module(core_emit_gate)."), "{}", output);
    assert!(output.contains("value() ->"), "{}", output);
}

#[test]
fn core_module_syntax_bridge_emit_rejects_stale_core_identity() {
    let module = parse_module_as_syntax_output(
        r#"
module stale_core_gate.

pub value(): Int ->
1.
"#,
    )
    .expect("parse stale core gate fixture");
    let mut core = test_core_module_for_syntax(&module);
    core.module = "other_module".to_string();

    let error = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect_err("stale CoreIR identity should be rejected");

    assert!(error.contains("CoreIR module mismatch"), "{}", error);
}
