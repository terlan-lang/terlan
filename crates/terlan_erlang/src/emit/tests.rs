use std::collections::BTreeMap;

use super::{lower_core_expr_to_erlang, try_emit_core_module_to_erlang_with_syntax_bridge};
use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output, span::Span,
    SyntaxModuleOutput, SyntaxSourceKind,
};
use terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreModule, CoreModuleMetadata,
    CorePattern, CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreSourceIdentity,
    CoreTupleTypeElem, CoreType, CORE_IR_SCHEMA,
};

/// Builds a minimal syntax-aware CoreIR module for backend gate tests.
///
/// Inputs:
/// - `module`: parsed syntax-output fixture.
///
/// Output:
/// - CoreIR module with matching schema, module name, source identity, and
///   interface payload.
///
/// Transformation:
/// - Copies syntax-output identity into CoreIR and derives the public
///   interface through the existing HIR adapter; declaration vectors are
///   left empty because these tests exercise backend identity gating only.
fn test_core_module_for_syntax(module: &SyntaxModuleOutput) -> CoreModule {
    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: module.module_name.clone(),
        source: CoreSourceIdentity {
            source_kind: format!("{:?}", module.source_kind),
            syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
        },
        imports: Vec::new(),
        exports: Vec::new(),
        types: Vec::new(),
        functions: Vec::new(),
        constructors: Vec::new(),
        trait_conformances: Vec::new(),
        metadata: CoreModuleMetadata {
            interface_function_count: 0,
            interface_type_count: 0,
            constructor_count: 0,
            proof_readiness: terlan_typeck::CoreProofReadiness::NoExpressions,
            lean_covered_expr_count: 0,
            partial_expr_count: 0,
            proof_model_required_expr_count: 0,
            runtime_boundary_expr_count: 0,
            artifact_only_expr_count: 0,
            lean_covered_pattern_count: 0,
            partial_pattern_count: 0,
            proof_model_required_pattern_count: 0,
            runtime_boundary_pattern_count: 0,
            artifact_only_pattern_count: 0,
            typed_core_expr_count: 0,
            summary_only_expr_count: 0,
            typed_core_pattern_count: 0,
            summary_only_pattern_count: 0,
            typed_core_type_count: 0,
            summary_only_type_count: 0,
            checked_preservation_expr_count: 0,
            checked_preservation_pattern_count: 0,
            checked_preservation_expr_structural_count: 0,
            checked_preservation_pattern_structural_count: 0,
            checked_preservation_expr_no_runtime_bindings_count: 0,
            checked_preservation_pattern_no_runtime_bindings_count: 0,
            checked_preservation_expr_runtime_bindings_required_count: 0,
            checked_preservation_pattern_runtime_bindings_required_count: 0,
            resolved_constructor_call_identity_count: 0,
            resolved_constructor_chain_identity_count: 0,
            resolved_constructor_pattern_identity_count: 0,
            unresolved_constructor_call_candidate_count: 0,
            unresolved_constructor_chain_candidate_count: 0,
            unresolved_constructor_pattern_candidate_count: 0,
        },
        interface: syntax_module_output_to_interface(module),
    }
}

/// Builds a pure CoreIR string intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: string primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
fn test_string_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    test_primitive_intrinsic_call(intrinsic, args, return_type)
}

/// Builds a pure CoreIR primitive intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
fn test_primitive_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 0),
    }
}

/// Builds an effectful CoreIR runtime capability call for Erlang backend tests.
///
/// Inputs:
/// - `capability`: runtime capability identity under test.
/// - `args`: CoreIR argument expressions supplied to the capability.
/// - `return_type`: typed CoreIR result contract for the capability.
///
/// Output:
/// - CoreIR intrinsic call with an `io` effect set and empty source span.
///
/// Transformation:
/// - Wraps the runtime capability identity and arguments in the production
///   CoreIR intrinsic-call shape used by source lowering.
fn test_runtime_capability_call(
    capability: CoreRuntimeCapability,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(capability),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["io".to_string()],
        },
        span: Span::new(0, 0),
    }
}

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

/// Verifies `core.string.byte_size` lowers through UTF-8 binary normalization.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Test assertion over the rendered Erlang expression.
///
/// Transformation:
/// - Builds a CoreIR intrinsic call and checks the nested Erlang byte-size
///   operation used for the current backend.
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
/// - Parses Terlan source with `@compiler.intrinsic`, wraps it in the
///   transitional CoreIR syntax-bridge payload, emits Erlang, and checks
///   that the backend intrinsic lowering is used instead of the source
///   placeholder expression.
#[test]
fn compiler_intrinsic_annotation_replaces_string_placeholder_body() {
    let module = parse_module_as_syntax_output(
        r#"
module intrinsic_annotation_fixture.

@compiler.intrinsic {core.string.contains}
pub contains(value: String, pattern: String): Bool ->
false.
"#,
    )
    .expect("syntax output with intrinsic annotation");
    let core = test_core_module_for_syntax(&module);

    let emitted = try_emit_core_module_to_erlang_with_syntax_bridge(
        &core,
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("annotated intrinsic module should emit");

    assert!(emitted.contains("string:find(Value, Pattern)"));
    assert!(emitted.contains("'nomatch'"));
    assert!(!emitted.contains("contains(Value, Pattern) ->\n    false."));
}

/// Verifies primitive Terlan type names map to BEAM specs.
///
/// Inputs:
/// - None; exercises the backend type-name mapper directly.
///
/// Output:
/// - Test passes when source-level `String` and transitional `Text` both lower
///   to BEAM `binary()` while `String` remains available as its own
///   frontend/CoreIR name.
///
/// Transformation:
/// - Converts primitive source type names into Erlang type-spec spelling.
#[test]
fn maps_string_primitive_to_binary_spec() {
    assert_eq!(super::map_type_name("String"), "binary()");
    assert_eq!(super::map_type_name("Text"), "binary()");
    assert_eq!(super::map_type_name("Binary"), "binary()");
}

/// Verifies CoreIR function-value invocation lowers to Erlang fun apply.
///
/// Inputs:
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

#[test]
fn formal_syntax_output_direct_emit_rejects_structural_alias_constructor_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module structural_alias_constructor_emit.

pub type Pair =
{left: Int, right: Int}.

pub make(): Pair ->
Pair(1, 2).
"#,
    )
    .expect("parse structural alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "structural aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_structural_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module pairs.

pub type Pair =
{left: Int, right: Int}.
"#,
    )
    .expect("parse structural alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_structural_alias_constructor_emit.

import pairs.{Pair}.

pub make(): Pair ->
Pair(1, 2).
"#,
    )
    .expect("parse imported structural alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported structural aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_map_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module props.

pub type Props =
#{name := Binary}.
"#,
    )
    .expect("parse map alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_map_alias_constructor_emit.

import props.{Props}.

pub make(name: Binary): Props ->
Props(#{name = name}).
"#,
    )
    .expect("parse imported map alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported map aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_union_alias_constructor_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module union_alias_constructor_emit.

pub type None =
:none | :empty.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse union alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "union aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_union_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module options.

pub type None =
:none | :empty.
"#,
    )
    .expect("parse union alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_union_alias_constructor_emit.

import options.{None}.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse imported union alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported union aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_union_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module union_alias_pattern_emit.

pub type None =
:none | :empty.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse union alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "union alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_union_alias_constructor_pattern_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module options.

pub type None =
:none | :empty.
"#,
    )
    .expect("parse union alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_union_alias_pattern_emit.

import options.{None}.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse imported union alias pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported union alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_list_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module list_alias_pattern_emit.

pub type Items[T] =
List[T].

pub unwrap(input: Items[Int]): Dynamic ->
case input {
    Items(values) -> values
}.
"#,
    )
    .expect("parse list alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "list alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_structural_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module structural_alias_pattern_emit.

pub type Pair =
{left: Int, right: Int}.

pub left(input: Pair): Int ->
case input {
    Pair(left, _right) -> left
}.
"#,
    )
    .expect("parse structural alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "structural alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_map_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module map_alias_pattern_emit.

pub type Props =
#{name := Binary}.

pub name(input: Props): Binary ->
case input {
    Props(values) -> values
}.
"#,
    )
    .expect("parse map alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "map alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_list_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module items.

pub type Items[T] =
List[T].
"#,
    )
    .expect("parse list alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_list_alias_constructor_emit.

import items.{Items}.

pub make(values: List[Int]): Items[Int] ->
Items(values).
"#,
    )
    .expect("parse imported list alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported list aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_nullary_literal_alias_constructor_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module nullary_literal_alias_constructor_emit.

pub type None =
:none.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse nullary literal alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "nullary literal alias constructor calls should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_nullary_literal_alias_constructor_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module literals.

pub type None =
:none.
"#,
    )
    .expect("parse literal alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_nullary_literal_alias_constructor_emit.

import literals.{None}.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse imported literal alias consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported nullary literal alias constructor calls should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quoted_atom_type_aliases() {
    let module = parse_module_as_syntax_output(
        r#"
module quoted_atom_alias_emit.

pub type ModuleAtom =
:'Elixir.Module'.

pub classify(value: ModuleAtom): Dynamic ->
case value {
    ModuleAtom -> :ok
}.
"#,
    )
    .expect("parse quoted atom alias emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quoted atom alias should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-type module_atom() :: 'Elixir.Module'."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("'Elixir.Module' -> ok"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_literal_alias_constructor_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_pattern_emit.

pub type Ok[T] =
{:ok, value: T}.

pub type None =
:none.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :none
}.
"#,
    )
    .expect("parse alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();
    assert!(output.contains("-type ok(T) :: {ok, T}."));
    assert!(output.contains("-type none() :: 'none'."));
    assert!(output.contains("'none' -> 'none'"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_literal_alias_values() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_value_emit.

pub type None =
:none.

pub none(): None ->
None.
"#,
    )
    .expect("parse alias value emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("literal alias values should lower directly from syntax output")
    .render();

    assert!(output.contains("-type none() :: 'none'."));
    assert!(
        output.contains("none() ->\n    'none'."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_imported_literal_alias_constructor_patterns() {
    let provider = parse_module_as_syntax_output(
        r#"
module literals.

pub type None =
:none.
"#,
    )
    .expect("parse literal alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_alias_pattern_emit.

import literals.{None}.

pub unwrap(input: None): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse imported literal alias pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("imported literal alias patterns should lower directly from syntax output")
    .render();

    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_preserves_type_and_function_docs() {
    let module = parse_module_as_syntax_output(
        r#"
//! Module docs.
//! Second module line.

module syntax_output_docs_emit.

/// Status value.
pub type Status = :ok.

/// Adds one.
pub add(x: Int): Int ->
x + 1.
"#,
    )
    .expect("parse syntax output docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("docs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-moduledoc \"Module docs.\\nSecond module line.\"."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Status value.\".\n\n-type status() :: ok."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Adds one.\".\n\n-spec add(integer()) -> integer()."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_preserves_struct_and_field_docs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_docs_emit.

/// A user account.
pub struct User {
/// Stable internal ID.
id: Int,

/// Display name.
name: Text
}.
"#,
    )
    .expect("parse syntax output struct docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct docs should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(output.contains("-doc \"A user account.\"."));
    assert!(output.contains("id % Stable internal ID."));
    assert!(output.contains("name % Display name."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_alias_constructor_subset() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_emit.

pub type Ok[T] =
{:ok, value: T}.

pub make(value: Int): Dynamic ->
Ok(value).

pub unwrap(input: Dynamic): Dynamic ->
case input {
    Ok(value) -> value
}.
"#,
    )
    .expect("parse syntax output emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-type ok(T) :: {ok, T}."));
    assert!(output.contains("make(Value) ->\n    {ok, Value}."));
    assert!(output.contains("{ok, Value} -> Value"));
}

#[test]
fn formal_syntax_output_direct_emit_try_api_uses_direct_lowering() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_emit.

pub id(value: Int): Int ->
value.
"#,
    )
    .expect("parse syntax output try emit fixture");

    let output = super::try_emit_syntax_module_output_to_erlang(&module)
        .expect("formal try emit should lower directly from syntax output");

    assert!(output.contains("-module(syntax_output_try_emit)."));
    assert!(output.contains("-export([id/1])."));
    assert!(output.contains("id(Value) ->\n    Value."));
}

#[test]
fn formal_syntax_output_direct_emit_ignores_source_export_payloads() {
    let mut module = parse_interface_module_as_syntax_output(
        r#"
module syntax_output_source_export_payload.

export ghost/1.
"#,
    )
    .expect("parse interface export payload fixture");
    module.source_kind = SyntaxSourceKind::Module;

    let output = super::try_emit_syntax_module_output_to_erlang(&module)
        .expect("module-mode export payload should still lower as an empty module");

    assert!(output.contains("-module(syntax_output_source_export_payload)."));
    assert!(!output.contains("-export([ghost/1])."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_if_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_if_emit.

pub choose(flag: Bool): Int ->
if {
    flag -> 1;
    true -> 0
}.
"#,
    )
    .expect("parse syntax output if fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("if expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("choose(Flag) ->\n    if\n    Flag -> 1;\n    true -> 0\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_receive_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_receive_emit.

pub wait(): Int ->
receive {
    {:ok, value} -> value;
    :stop -> 0
}.
"#,
    )
    .expect("parse syntax output receive fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("receive expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("wait() ->\n    receive\n    {ok, Value} -> Value;\n    'stop' -> 0\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_expr_emit.

pub wait(): Int ->
try risky() {
    {:ok, value} -> value
catch
    :error -> 0
}.

risky(): {:ok, Int} ->
{:ok, 1}.
"#,
    )
    .expect("parse syntax output try expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("try expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "wait() ->\n    try risky()\nof\n    {ok, Value} -> Value\n\ncatch\n    error -> 0\nend."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_after_cleanup() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_after_emit.

pub wait(): Int ->
try risky() {
after
    0 -> 1
}.

risky(): Int ->
1.
"#,
    )
    .expect("parse syntax output try after expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    let module = output.expect("try-after should lower directly from syntax output");

    let source = module.render();
    assert!(source.contains("after\n    0 -> 1"), "output:\n{}", source);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_receive_after_timeout() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_receive_after_emit.

pub wait(): Int ->
receive {
    {:ok, value} -> value;
after
    0 -> 1
}.
"#,
    )
    .expect("parse syntax output receive after expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    let module = output.expect("receive-after should lower directly from syntax output");
    let source = module.render();
    assert!(source.contains("after\n    0 -> 1"), "output:\n{}", source);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_unary_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_unary_emit.

pub flip(flag: Bool): Bool ->
not flag.

pub negate(value: Int): Int ->
-value.
"#,
    )
    .expect("parse syntax output unary expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("unary expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("flip(Flag) ->\n    not Flag."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("negate(Value) ->\n    -Value."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_remote_fun_ref_source_syntax() {
    let parsed = parse_module_as_syntax_output(
        r#"
module syntax_output_remote_fun_ref_emit.

pub ref(): Dynamic ->
fun math:double/1.
"#,
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_macro_exprs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_macro_emit.

pub module_name(): Dynamic ->
?MODULE.

pub compare(a: Int, b: Int): Dynamic ->
?assert_equal(a, b).
"#,
    )
    .expect("parse syntax output macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("macro exprs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_name() ->\n    ?MODULE."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("compare(A, B) ->\n    ?assert_equal(A, B)."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_raw_macro_exprs_without_resolution() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_macro_emit.

pub query(): Dynamic ->
sql{select * from users}.
"#,
    )
    .expect("parse syntax output raw macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "raw macro expr should require macro resolution before direct emit"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_constructor_chain() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_chain_emit.

pub type User = Dynamic.
pub constructor User {
(id: Int, name: Binary): Dynamic ->
    id
}.

pub demo(id: Int, name: Binary): Dynamic ->
User(id, name) with Admin { id = id, name = name }.
"#,
    )
    .expect("parse syntax output constructor chain fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_some(),
        "constructor chain should lower during direct syntax emit"
    );

    let source = output
        .as_ref()
        .expect("output exists because test expects lowering")
        .render();
    assert!(
        source.contains("begin\n"),
        "expected constructor chain to lower to sequenced derived shape: {}",
        source
    );
    assert!(
        source.contains("{'Admin', Id, Name}"),
        "expected constructed derived tuple to be emitted: {}",
        source
    );
    assert!(
        !source.contains("#admin"),
        "constructor extension must not emit undeclared Erlang records: {}",
        source
    );
}

#[test]
fn formal_syntax_output_direct_emit_maps_binary_ops_without_ast_enum() {
    assert_eq!(super::lower_syntax_binary_op_render("+"), "+");
    assert_eq!(super::lower_syntax_binary_op_render("=="), "=:=");
    assert_eq!(super::lower_syntax_binary_op_render("=:="), "=:=");
    assert_eq!(super::lower_syntax_binary_op_render("<="), "=<");
    assert_eq!(super::lower_syntax_binary_op_render("div"), "div");
    assert_eq!(super::lower_syntax_binary_op_render("!"), "!");
}

#[test]
fn formal_syntax_output_direct_emit_lowers_pipe_forward() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_pipe_emit.

pub add(value: Int, amount: Int): Int ->
value + amount.

pub demo(value: Int): Int ->
value |> add(1).
"#,
    )
    .expect("parse syntax output pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("pipe subset should lower directly from syntax output")
    .render();

    assert!(output.contains("demo(Value) ->\n    add(Value, 1)."));
    assert!(!output.contains("|>"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_keyword_expr_pipe_forward() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_keyword_pipe_emit.

pub inspect(value: Int): Int ->
value.

pub demo(option: Dynamic): Int ->
case option {
    :none -> 0;
    value -> value
} |> inspect().
"#,
    )
    .expect("parse syntax output keyword pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("keyword pipe should lower directly from syntax output")
    .render();

    assert!(
        output.contains("demo(Option) ->\n    inspect(case Option of"),
        "output:\n{}",
        output
    );
    assert!(!output.contains("|>"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_function_clause_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_function_guard_emit.

pub abs(value) when value < 0 ->
0 - value;
abs(value) ->
value.
"#,
    )
    .expect("parse syntax output function guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("function guards should lower directly from syntax output")
    .render();

    assert!(
        output.contains("abs(Value) when Value < 0 ->\n    0 - Value;"),
        "output:\n{}",
        output
    );
    assert!(output.contains("abs(Value) ->\n    Value."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_send_operator() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_send_emit.

pub deliver(pid: Pid, message: Int): Dynamic ->
pid ! message.
"#,
    )
    .expect("parse syntax output send fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("send operator should lower directly from syntax output")
    .render();

    assert!(
        output.contains("deliver(Pid, Message) ->\n    Pid ! Message."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_case_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_case_guard_emit.

pub classify(value: Int): Int ->
case value {
    x when x > 0 -> x;
    _ -> 0
}.
"#,
    )
    .expect("parse syntax output case guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("case guard should lower directly from syntax output")
    .render();

    assert!(output.contains("X when X > 0 -> X"), "output:\n{}", output);
    assert!(output.contains("_ -> 0"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_raw_atom_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_atom_pattern_emit.

pub classify(value: Dynamic): Dynamic ->
case value {
    :none -> :ok;
    :empty -> :ok;
    other -> other
}.
"#,
    )
    .expect("parse syntax output raw atom pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("raw atom patterns should lower directly from syntax output")
    .render();

    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
    assert!(output.contains("'empty' -> ok"), "output:\n{}", output);
    assert!(output.contains("Other -> Other"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quoted_atom_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_quoted_atom_emit.

pub module_atom(): Dynamic ->
:'Elixir.Module'.

pub classify(value: Dynamic): Dynamic ->
case value {
    :'some atom' -> :ok;
    :none -> :ok
}.
"#,
    )
    .expect("parse syntax output quoted atom fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quoted atoms should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_atom() ->\n    'Elixir.Module'."),
        "output:\n{}",
        output
    );
    assert!(output.contains("'some atom' -> ok"), "output:\n{}", output);
    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_bool_literals_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_bool_literal_emit.

pub negate(value: Bool): Bool ->
case value {
    true -> false;
    false -> true
}.
"#,
    )
    .expect("parse syntax output bool literal fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("bool literals should lower directly from syntax output")
    .render();

    assert!(output.contains("true -> false"), "output:\n{}", output);
    assert!(output.contains("false -> true"), "output:\n{}", output);
}

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

/// Verifies compiler-owned BEAM runtime calls still lower after method-call
/// syntax was added.
///
/// Inputs:
/// - A formal syntax-output module containing `erlang.integer_to_list(value)`.
///
/// Output:
/// - Test passes when direct Erlang lowering emits an Erlang remote call.
///
/// Transformation:
/// - Parses the source through canonical syntax output, where
///   `erlang.integer_to_list(...)` is method-shaped syntax, then verifies
///   the Erlang syntax bridge reclassifies the known backend runtime
///   root without enabling arbitrary receiver-method lowering.
#[test]
fn formal_syntax_output_direct_emit_lowers_known_backend_runtime_method_shape() {
    let module = parse_module_as_syntax_output(
        r#"
module backend_runtime_method_shape.

pub render(value: Int): String ->
erlang.integer_to_list(value).
"#,
    )
    .expect("parse backend runtime method-shaped call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower known backend runtime method shape")
    .render();

    assert!(
        output.contains("render(Value) ->\n    erlang:integer_to_list(Value)."),
        "output:\n{}",
        output
    );
}

/// Verifies local receiver-method calls do not lower through the remote-call
/// syntax bridge path.
///
/// Inputs:
/// - A formal syntax-output module containing `user.display_name()`, where
///   `user` is a function parameter.
///
/// Output:
/// - Test passes when direct Erlang lowering rejects the module because
///   semantic receiver-method resolution has not run.
///
/// Transformation:
/// - Parses the canonical method-call suffix and checks that the Erlang
///   syntax bridge treats local receivers differently from module
///   roots.
#[test]
fn formal_syntax_output_direct_emit_does_not_lower_local_receiver_method_as_remote_call() {
    let module = parse_module_as_syntax_output(
        r#"
module local_receiver_method_shape.

pub render(user: User): String ->
user.display_name().
"#,
    )
    .expect("parse local receiver method-shaped call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(output.is_none());
}

/// Verifies Erlang syntax lowering preserves receiver mutability metadata.
///
/// Inputs:
/// - A syntax-output module containing one mutable receiver method and one
///   immutable receiver method for the same receiver type.
///
/// Output:
/// - Test passes when the lowering context marks only the mutable method as
///   mutable while preserving ordinary receiver-method lookup behavior.
///
/// Transformation:
/// - Builds the direct syntax-output lowering context and inspects the internal
///   receiver-method inventory that later mutable rebinding lowering will use.
#[test]
fn formal_syntax_output_lowering_context_preserves_receiver_mutability() {
    let module = parse_module_as_syntax_output(
        r#"
module receiver_mutability_context.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    Unit.

pub (map: Map) size(): Int ->
    map.size.
"#,
    )
    .expect("parse receiver mutability context fixture");

    let ctx = super::syntax::SyntaxLowerCtx::new(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        ctx.receiver_method_target("Map", "put", 0)
            .expect("put receiver target")
            .mutable
    );
    assert!(
        !ctx.receiver_method_target("Map", "size", 0)
            .expect("size receiver target")
            .mutable
    );
}

/// Verifies mutable receiver pipes bind the backend-updated receiver.
///
/// Inputs:
/// - A syntax-output module with one mutable receiver method and one function
///   using `map |> put()`.
///
/// Output:
/// - Test passes when direct Erlang lowering emits a backend-local binding from
///   `put(Map)` and returns that binding as the pipe expression value.
///
/// Transformation:
/// - Runs the syntax-output Erlang bridge directly, bypassing formal pipeline
///   typecheck gates, and inspects the rendered Erlang source for the hidden
///   mutable receiver threading convention.
#[test]
fn formal_syntax_output_direct_emit_lowers_mutable_receiver_pipe_to_updated_binding() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_pipe_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    map.

pub run(map: Map): Map ->
    map |> put().
"#,
    )
    .expect("parse mutable receiver pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver pipe should lower directly from syntax output")
    .render();

    assert!(
        output.contains("_TerlanMutReceiver = put(Map)"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains(
            "run(Map) ->\n    begin\n    _TerlanMutReceiver = put(Map),\n    _TerlanMutReceiver\nend."
        ),
        "output:\n{}",
        output
    );
    assert!(!output.contains("|>"), "output:\n{}", output);
}

/// Verifies sequences thread mutable receiver updates into later expressions.
///
/// Inputs:
/// - A syntax-output module with a mutable receiver method, an immutable
///   receiver method, and a sequence `map.put(); map.size()`.
///
/// Output:
/// - Test passes when the mutable call result is bound once and the following
///   receiver method call uses the updated binding instead of the original
///   source parameter.
///
/// Transformation:
/// - Runs direct syntax-output Erlang lowering and inspects the rendered
///   sequence binding convention used before formal build gates enable mutable
///   receiver execution.
#[test]
fn formal_syntax_output_direct_emit_lowers_mutable_receiver_sequence_to_rebinding() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_sequence_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    map.

pub (map: Map) size(): Int ->
    map.size.

pub run(map: Map): Int ->
    map.put(); map.size().
"#,
    )
    .expect("parse mutable receiver sequence fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver sequence should lower directly from syntax output")
    .render();

    assert!(
        output.contains("_TerlanMutReceiver0 = put(Map)"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("size(_TerlanMutReceiver0)"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_qualified_type_specs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_qualified_specs.

pub id(value: users.UserId): users.UserId ->
value.

pub boxed(value: users.Box[Int]): users.Box[Int] ->
value.

pub comparison(value: std.core.Ordering.Comparison): std.core.Ordering.Comparison ->
value.
"#,
    )
    .expect("parse syntax output qualified type fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("qualified type specs should lower directly from syntax output")
    .render();

    assert!(output.contains("-spec id(users:user_id()) -> users:user_id()."));
    assert!(output.contains("-spec boxed(users:box(integer())) -> users:box(integer())."));
    assert!(output.contains(
        "-spec comparison(std_core_ordering:comparison()) -> std_core_ordering:comparison()."
    ));
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
fn formal_syntax_output_direct_emit_lowers_opaque_constructors_and_phantom_specs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_queue_specs.

pub opaque type Queue[T] =
Term.

pub empty(): Queue[T] ->
Queue(queue.new()).

pub from_term(value: Term): Queue[T] ->
Queue(value).

pub len(queue_value: Queue[T]): Int ->
queue.len(queue_value).
"#,
    )
    .expect("parse syntax output opaque queue fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("opaque constructors should lower directly from syntax output")
    .render();

    assert!(output.contains("-opaque queue(_T) :: term()."));
    assert!(output.contains("-spec from_term(term()) -> queue(_T)."));
    assert!(output.contains("-spec len(queue(_T)) -> integer()."));
    assert!(output.contains("empty() ->\n    queue:new()."));
    assert!(output.contains("from_term(Value) ->\n    Value."));
    assert!(!output.contains("Queue("), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_rejects_local_opaque_constructor_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_local_opaque_pattern_emit.

pub opaque type UserId =
Int.

pub unwrap(input: UserId): Int ->
case input {
    UserId(value) -> value
}.
"#,
    )
    .expect("parse local opaque pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "local opaque constructor patterns should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_opaque_constructor_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module users.

pub opaque type UserId =
Int.
"#,
    )
    .expect("parse opaque provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module syntax_output_imported_opaque_emit.

import users.{UserId}.

pub make(value: Int): UserId ->
UserId(value).
"#,
    )
    .expect("parse imported opaque consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported opaque constructor calls should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_opaque_constructor_patterns() {
    let provider = parse_module_as_syntax_output(
        r#"
module users.

pub opaque type UserId =
Int.
"#,
    )
    .expect("parse opaque provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module syntax_output_imported_opaque_pattern_emit.

import users.{UserId}.

pub unwrap(input: UserId): Int ->
case input {
    UserId(value) -> value
}.
"#,
    )
    .expect("parse imported opaque pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported opaque constructor patterns should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_maps_and_funs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_container_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output container fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("container subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
    assert!(output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_map_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_map_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.
"#,
    )
    .expect("parse syntax output map fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("map expressions and patterns should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_anonymous_fun_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fun_emit.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output anonymous fun fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("anonymous fun should lower directly from syntax output")
    .render();

    assert!(
        output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_fixed_array_indexes() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fixed_array_index_emit.

pub second(): Dynamic ->
#[1, 2, 3][1].
"#,
    )
    .expect("parse syntax output fixed array index fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("fixed array index should lower directly from syntax output")
    .render();

    assert!(
        output.contains("second() ->\n    element((1) + 1, {1, 2, 3})."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_comprehensions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_emit.

pub increment(values: List[Int]): List[Int] ->
[value + 1 | value <- values].
"#,
    )
    .expect("parse syntax output list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains("increment(Values) ->\n    [Value + 1 || Value <- Values]."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_stacked_list_comprehension_filters() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_filter_emit.

pub selected(values: List[Int]): List[Int] ->
[value | value <- values, value > 0, value < 10].
"#,
    )
    .expect("parse stacked-filter list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("stacked-filter list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "selected(Values) ->\n    [Value || Value <- Values, Value > 0 andalso Value < 10]."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_cons_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_cons_emit.

pub prepend(head: Int, tail: List[Int]): List[Int] ->
[head | tail].

pub first(list: List[Int]): Int ->
case list {
    [head | _tail] -> head
}.
"#,
    )
    .expect("parse syntax output list cons fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list cons subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("prepend(Head, Tail) ->\n    [Head|Tail]."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("[Head|_tail] -> Head"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_constructs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_construct_emit.

pub make(id: Int, name: Text): Dynamic ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output record construct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record construct should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quote_and_unquote() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_quote_emit.

pub quoted(value: Int): Dynamic ->
quote unquote(value).
"#,
    )
    .expect("parse syntax output quote fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quote subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("quoted(Value) ->\n    quote unquote(Value)."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_structs_with_defaults() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_emit.

pub struct User {
id: Int,
name: Text,
status: Dynamic = :active
}.

pub make(id: Int, name: Text): User ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output struct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(
        output.contains("-record(user, {id, name, status = 'active'})."),
        "output:\n{}",
        output
    );
    assert!(output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_field_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
user.name.
"#,
    )
    .expect("parse syntax output struct field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct field access should lower directly from syntax output")
    .render();

    assert!(output.contains("username(User) ->\n    User#user.name."));
    assert!(
        output.find("-record(user").unwrap_or(usize::MAX)
            < output.find("-type user").unwrap_or(usize::MAX),
        "record declarations must appear before types that reference them:\n{}",
        output
    );
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_updates() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_update_emit.

pub struct User {
id: Int,
name: Text
}.

pub rename(user: User, name: Text): User ->
user#User{name = name}.
"#,
    )
    .expect("parse syntax output record update fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record update should lower directly from syntax output")
    .render();

    assert!(
        output.contains("rename(User, Name) ->\n    User#user{name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_pattern_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
case user {
    #User{name = name} -> name
}.
"#,
    )
    .expect("parse syntax output record pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record pattern should lower directly from syntax output")
    .render();

    assert!(
        output.contains("#user{name = Name} -> Name"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_headers() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_header_emit.

/// A user account.
pub struct User {
id: Int,
name: Text = <<"guest">>
}.
"#,
    )
    .expect("parse syntax output struct header fixture");

    let output = super::lower_syntax_struct_headers_to_hrl(&module)
        .expect("struct headers should lower directly from syntax output");

    assert!(output.contains("-doc \"A user account.\"."));
    assert!(output.contains("-record(user, {id, name = <<\"guest\">>})."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_explicit_constructors() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_emit.

pub type Range =
Dynamic.

pub constructor Range {
(start: Int, stop: Int, step: Int = 1): Range ->
    {:range, start, stop, step}
}.

pub make(start: Int, stop: Int): Range ->
Range(start, stop).

pub first(value: Range): Int ->
case value {
    Range(start, stop, step) -> start
}.
"#,
    )
    .expect("parse syntax output constructor fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("explicit constructor subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([first/1, make/2, typer_ctor_range_3/3])."));
    assert!(
        output.contains(
            "typer_ctor_range_3(Start, Stop, Step) ->\n    {'range', Start, Stop, Step}."
        ),
        "output:\n{}",
        output
    );
    assert!(output.contains("make(Start, Stop) ->\n    typer_ctor_range_3(Start, Stop, 1)."));
    assert!(output.contains("{'range', Start, Stop, Step} -> Start"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_varargs_constructors() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_varargs_emit.

pub type Items[T] =
List[T].

pub constructor Items[T] {
(...values: T): Items[T] ->
    values
}.

pub from_args(a: Int, b: Int): Items[Int] ->
Items(a, b).
"#,
    )
    .expect("parse syntax output varargs constructor fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("varargs constructor subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([from_args/2, typer_ctor_items_varargs_0/1])."));
    assert!(output.contains("-spec typer_ctor_items_varargs_0([T]) -> items(T)."));
    assert!(output.contains("typer_ctor_items_varargs_0(Values) ->\n    Values."));
    assert!(
        output.contains("from_args(A, B) ->\n    typer_ctor_items_varargs_0([A, B])."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_constructor_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_field_emit.

pub struct User {
name: Text
}.

pub type Named =
Dynamic.

pub constructor Named {
(user: User): Named ->
    {:named, user.name}
}.
"#,
    )
    .expect("parse syntax output constructor field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("constructor field access should lower directly from syntax output")
    .render();

    assert!(
        output.contains("typer_ctor_named_1(User) ->\n    {'named', User#user.name}."),
        "output:\n{}",
        output
    );
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_native_raw_declarations() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_native_emit.

native core module ArrayNative {
#[nif(normal)]
length[T](value: Array[T]): Int.
}
"#,
    )
    .expect("parse syntax output native raw fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("native raw subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([length/1])."));
    assert!(output.contains("-on_load(load/0)."));
    assert!(output.contains("\"ArrayNative.so\""));
    assert!(output.contains("length(A1) ->\n    erlang:nif_error(nif_not_loaded)."));
}

#[test]
fn formal_syntax_output_direct_emit_embeds_file_imports() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_file_import_emit.

import file "./hello.html" as HelloHtml.

pub page(): Binary ->
HelloHtml.
"#,
    )
    .expect("parse syntax output file import fixture");

    let mut files = BTreeMap::new();
    files.insert("HelloHtml".to_string(), b"<h1>Hello</h1>".to_vec());

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &files,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("file import subset should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->\n    <<60,104,49,62,72,101,108,108,111,60,47,104,49,62>>."));
}

#[test]
fn formal_syntax_output_direct_emit_embeds_css_imports() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_css_import_emit.

import css "./style.css" as PageCss.

pub css(): Binary ->
PageCss.
"#,
    )
    .expect("parse syntax output css import fixture");

    let mut files = BTreeMap::new();
    files.insert("PageCss".to_string(), b"main{display:block}".to_vec());

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &files,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("css import subset should lower directly from syntax output")
    .render();

    assert!(output.contains(
        "css() ->\n    <<109,97,105,110,123,100,105,115,112,108,97,121,58,98,108,111,99,107,125>>."
    ));
}

#[test]
fn formal_syntax_output_direct_emit_embeds_markdown_import_fields() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_markdown_import_emit.

import markdown "./posts/hello.md" as HelloPost.

pub raw(): Binary ->
HelloPost.raw.

pub html(): Html[:none] ->
HelloPost.html.
"#,
    )
    .expect("parse syntax output markdown import fixture");

    let mut markdown = BTreeMap::new();
    markdown.insert(
        "HelloPost".to_string(),
        terlan_html::parse_markdown("# Hello\n", "posts/hello.md").expect("markdown"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &markdown,
    )
    .expect("markdown import subset should lower directly from syntax output")
    .render();

    assert!(output.contains("raw() ->\n    <<35,32,72,101,108,108,111,10>>."));
    assert!(
        output.contains("html() ->\n    <<60,104,49,62,72,101,108,108,111,60,47,104,49,62,10>>.")
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_template_instantiation() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_emit.

template Page from "./page.tl.html" {
title: Text,
url: Text,
body: Html[:none]
}.

pub page(): Html[:none] ->
Page{
    title = <<"Hi & Bye">>,
    url = <<"/posts?tag=a&b=1">>,
    body = Html.raw(<<"<strong>ok</strong>">>)
}.
"#,
    )
    .expect("parse syntax output template fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(
            r#"<a href="{url}">{title}</a><main>{body}</main>"#,
            "page.tl.html",
        )
        .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("template subset should lower directly from syntax output")
    .render();

    assert!(output.contains("page() ->"));
    assert!(output.contains("typer_html:escape(<<\"/posts?tag=a&b=1\">>)"));
    assert!(output.contains("typer_html:escape(<<\"Hi & Bye\">>)"));
    assert!(output.contains("<<\"<strong>ok</strong>\">>"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_template_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_template_field_emit.

pub struct User {
name: Text
}.

template Page from "./page.tl.html" {
title: Text
}.

pub page(user: User): Html[:none] ->
Page{
    title = user.name
}.
"#,
    )
    .expect("parse syntax output template field fixture");

    let mut templates = BTreeMap::new();
    templates.insert(
        "Page".to_string(),
        terlan_html::parse_html_template(r#"<h1>{title}</h1>"#, "page.tl.html")
            .expect("parse template"),
    );

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &templates,
        &BTreeMap::new(),
    )
    .expect("template field access should lower directly from syntax output")
    .render();

    assert!(output.contains("typer_html:escape(User#user.name)"));
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_blocks() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_block_emit.

pub view(Title: Text, Body: Binary): Html[:none] ->
html {
    <section class={["hero", "compact"]} data-title={Title}>
        <h1>{Title}</h1>
        {Html.raw(Body)}
    </section>
}.
"#,
    )
    .expect("parse syntax output html block fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html block subset should lower directly from syntax output")
    .render();

    assert!(output.contains("view(Title, Body) ->"));
    assert!(output.contains("<<\"<section class=\\\"hero compact\\\"\">>"));
    assert!(output.contains("typer_html:escape(Title)"));
    assert!(output.contains("Body"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_dynamic_html_attrs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_dynamic_attr_emit.

pub type Route = :home.

pub link(to: Route): Html[:none] ->
html { <a href={route.to_path(to)}>Open</a> }.
"#,
    )
    .expect("parse syntax output html dynamic attr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html dynamic attributes should lower directly from syntax output")
    .render();

    assert!(output.contains("<<\"<a\">>"));
    assert!(output.contains("<<\" href=\\\"\">>"));
    assert!(
        output.contains("typer_html:escape(route:to_path(To))"),
        "output:\n{}",
        output
    );
    assert!(output.contains("<<\"\\\"\">>"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_raw_html_children_without_escape() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_raw_child_emit.

pub view(trusted: Binary): Html[:none] ->
html {
    <main>{Html.raw(trusted)}</main>
}.
"#,
    )
    .expect("parse syntax output html raw child fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("raw html children should lower directly from syntax output")
    .render();

    assert!(
        output.contains("view(Trusted) ->\n    [<<\"<main>\">>, Trusted, <<\"</main>\">>]."),
        "output:\n{}",
        output
    );
    assert!(
        !output.contains("typer_html:escape(Trusted)"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_list_comprehension_children() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_for_emit.

pub view(users: List[Text]): Html[:none] ->
html {
    <ul>{for user <- users {
        <li>{user}</li>
    }}</ul>
}.
"#,
    )
    .expect("parse syntax output html list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html list comprehension should lower directly from syntax output")
    .render();

    assert!(output
        .contains("[[<<\"<li>\">>, typer_html:escape(User), <<\"</li>\">>] || User <- Users]"));
    assert!(
        !output.contains("typer_html:escape(["),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_case_children_per_branch() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_case_emit.

pub view(admin: Bool, name: Text): Html[:none] ->
html {
    <main>{case admin {
        true -> <span>Admin</span>;
        false -> name
    }}</main>
}.
"#,
    )
    .expect("parse syntax output html case fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html case child should lower directly from syntax output")
    .render();

    assert!(
        output.contains("true -> [<<\"<span>\">>, <<\"Admin\">>, <<\"</span>\">>]"),
        "output:\n{}",
        output
    );
    assert!(output.contains("false -> typer_html:escape(Name)"));
    assert!(
        !output.contains("typer_html:escape(case Admin of"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_html_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_html_field_emit.

pub struct User {
name: Text
}.

pub view(user: User): Html[:none] ->
html {
    <section data-name={user.name}>
        <h1>{user.name}</h1>
    </section>
}.
"#,
    )
    .expect("parse syntax output html field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("html field access should lower directly from syntax output")
    .render();

    assert!(output.contains("typer_html:escape(User#user.name)"));
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_handles_trait_and_template_decls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_noop_decls_emit.

pub trait Show[A] {
show(value: A): Text.
}.

template Page from "./page.tl.html" {
title: Text
}.

pub id(value: Int): Int ->
value.
"#,
    )
    .expect("parse syntax output no-op decl fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("trait and template declarations should lower directly from syntax output")
    .render();

    assert!(output.contains("%% trait Show."));
    assert!(output.contains("-export([id/1])."));
    assert!(output.contains("id(Value) ->\n    Value."));
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
