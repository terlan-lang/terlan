use terlan_syntax::syntax_output::SyntaxAnnotationOutput;
use terlan_syntax::{SyntaxDeclarationOutput, SyntaxParamOutput};
use terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePattern,
    CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreTupleTypeElem, CoreType,
};

use super::erl::*;
use super::{
    lower_syntax_binary_op, lower_syntax_unary_op, sanitize_erlang_fn_name, sanitize_erlang_var,
};

/// Lowers a supported backend-neutral CoreIR expression into an Erlang AST expression.
///
/// Inputs:
/// - `expr`: CoreIR expression produced after syntax-output lowering.
///
/// Output:
/// - `Some(ErlExpr)` when the expression belongs to the currently supported
///   Erlang CoreIR backend subset.
/// - `None` when the expression still needs a dedicated backend lowering rule.
///
/// Transformation:
/// - Maps backend-neutral CoreIR literals, calls, remote calls, operators,
///   tuples, lists, list cons cells, list comprehensions, fixed arrays, and
///   primitive intrinsics into the emitter's Erlang expression model without
///   consulting source syntax.
#[allow(dead_code)]
pub(super) fn lower_core_expr_to_erlang(expr: &CoreExpr) -> Option<ErlExpr> {
    match expr {
        CoreExpr::Int(value) => Some(ErlExpr::Int(*value)),
        CoreExpr::Float(value) => Some(ErlExpr::Float(value.clone())),
        CoreExpr::Binary(value) => Some(ErlExpr::Binary(value.clone())),
        CoreExpr::Atom(name) => Some(ErlExpr::Atom(name.clone())),
        CoreExpr::Var(name) => Some(ErlExpr::Var(sanitize_erlang_var(name))),
        CoreExpr::Tuple(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::Tuple),
        CoreExpr::List(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::List),
        CoreExpr::FixedArray(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::FixedArray),
        CoreExpr::ListCons { head, tail } => Some(ErlExpr::ListCons(
            Box::new(lower_core_expr_to_erlang(head)?),
            Box::new(lower_core_expr_to_erlang(tail)?),
        )),
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => Some(ErlExpr::ListComprehension {
            expr: Box::new(lower_core_expr_to_erlang(expr)?),
            pattern: lower_core_pattern_to_erlang(pattern)?,
            source: Box::new(lower_core_expr_to_erlang(source)?),
            guard: match guard.as_deref() {
                Some(guard) => Some(Box::new(lower_core_expr_to_erlang(guard)?)),
                None => None,
            },
        }),
        CoreExpr::Call { function, args } => Some(ErlExpr::Call {
            module: None,
            function: sanitize_erlang_fn_name(function),
            args: lower_core_exprs_to_erlang(args)?,
        }),
        CoreExpr::FunctionCall { callee, args } => Some(ErlExpr::Apply {
            callee: Box::new(lower_core_expr_to_erlang(callee)?),
            args: lower_core_exprs_to_erlang(args)?,
        }),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => Some(erl_remote_call(
            module,
            &sanitize_erlang_fn_name(function),
            lower_core_exprs_to_erlang(args)?,
        )),
        CoreExpr::UnaryOp { operator, operand } => Some(ErlExpr::UnaryOp {
            op: lower_syntax_unary_op(Some(operator)),
            expr: Box::new(lower_core_expr_to_erlang(operand)?),
        }),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => Some(ErlExpr::BinaryOp {
            op: lower_syntax_binary_op(Some(operator)),
            left: Box::new(lower_core_expr_to_erlang(left)?),
            right: Box::new(lower_core_expr_to_erlang(right)?),
        }),
        CoreExpr::Intrinsic(call) => lower_core_intrinsic_call_to_erlang(call),
        CoreExpr::Index { .. }
        | CoreExpr::Let { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::FieldAccess { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::ConstructorChain { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::ConstructorCall { .. }
        | CoreExpr::MutableReceiverCall { .. }
        | CoreExpr::Case { .. }
        | CoreExpr::Receive { .. }
        | CoreExpr::Try { .. }
        | CoreExpr::If { .. } => None,
        CoreExpr::Lam { params, body } => Some(ErlExpr::Fun(vec![ErlFunctionClause {
            patterns: lower_core_patterns_to_erlang(params)?,
            guard: None,
            body: lower_core_expr_to_erlang(body)?,
        }])),
    }
}

/// Lowers a list of CoreIR expressions into Erlang expressions.
///
/// Inputs:
/// - `args`: CoreIR expression slice to lower in order.
///
/// Output:
/// - `Some(Vec<ErlExpr>)` when every expression is supported by the current
///   Erlang CoreIR backend subset.
/// - `None` when any expression is outside the current subset.
///
/// Transformation:
/// - Applies `lower_core_expr_to_erlang` element-wise and preserves argument
///   order for call and intrinsic lowering.
fn lower_core_exprs_to_erlang(args: &[CoreExpr]) -> Option<Vec<ErlExpr>> {
    args.iter().map(lower_core_expr_to_erlang).collect()
}

/// Lowers CoreIR lambda parameter patterns into Erlang patterns.
///
/// Inputs:
/// - `patterns`: CoreIR lambda parameter patterns.
///
/// Output:
/// - Erlang patterns for the currently supported CoreIR pattern subset.
/// - `None` when a parameter uses a pattern outside the backend subset.
///
/// Transformation:
/// - Converts supported Core patterns into backend Erlang patterns without
///   introducing match helpers.
fn lower_core_patterns_to_erlang(patterns: &[CorePattern]) -> Option<Vec<ErlPattern>> {
    patterns.iter().map(lower_core_pattern_to_erlang).collect()
}

/// Lowers one CoreIR lambda parameter pattern into an Erlang pattern.
///
/// Inputs:
/// - `pattern`: CoreIR lambda parameter pattern.
///
/// Output:
/// - `Some(ErlPattern)` for direct variable, wildcard, literal, tuple, list,
///   and list-cons patterns.
/// - `None` for pattern forms outside the current CoreIR Erlang subset.
///
/// Transformation:
/// - Preserves direct parameter binding names with Erlang variable hygiene,
///   maps wildcard parameters to `_`, and recursively lowers simple
///   destructuring patterns that Erlang can represent natively.
fn lower_core_pattern_to_erlang(pattern: &CorePattern) -> Option<ErlPattern> {
    match pattern {
        CorePattern::Var(name) => Some(ErlPattern::Var(sanitize_erlang_var(name))),
        CorePattern::Wildcard => Some(ErlPattern::Wildcard),
        CorePattern::Int(value) => Some(ErlPattern::Int(*value)),
        CorePattern::Float(value) => Some(ErlPattern::Float(value.clone())),
        CorePattern::Atom(value) => Some(ErlPattern::Atom(value.clone())),
        CorePattern::Tuple(items) => Some(ErlPattern::Tuple(lower_core_patterns_to_erlang(items)?)),
        CorePattern::List(items) => Some(ErlPattern::List(lower_core_patterns_to_erlang(items)?)),
        CorePattern::ListCons { head, tail } => Some(ErlPattern::ListCons(
            Box::new(lower_core_pattern_to_erlang(head)?),
            Box::new(lower_core_pattern_to_erlang(tail)?),
        )),
        _ => None,
    }
}

/// Lowers a CoreIR intrinsic call into an Erlang expression.
///
/// Inputs:
/// - `call`: typed backend-neutral intrinsic call.
///
/// Output:
/// - `Some(ErlExpr)` for the currently supported primitive intrinsic set.
/// - `None` for malformed arities or intrinsic variants not handled by the
///   Erlang backend.
///
/// Transformation:
/// - Selects target runtime operations for primitive conversions, string
///   operations, and admitted runtime capabilities while keeping source-facing
///   `std` APIs and CoreIR registry keys backend neutral.
#[allow(dead_code)]
pub(super) fn lower_core_intrinsic_call_to_erlang(call: &CoreIntrinsicCall) -> Option<ErlExpr> {
    let args = lower_core_exprs_to_erlang(&call.args)?;
    match &call.id {
        CoreIntrinsicId::Primitive(intrinsic) => {
            lower_core_primitive_intrinsic_to_erlang(intrinsic, args)
        }
        CoreIntrinsicId::Runtime(capability) => match capability {
            CoreRuntimeCapability::ConsolePrintln => lower_runtime_console_println(args),
            CoreRuntimeCapability::FileExists => lower_runtime_file_exists(args),
            CoreRuntimeCapability::FileReadText => lower_runtime_file_read_text(args),
            CoreRuntimeCapability::FileWriteText => lower_runtime_file_write_text(args),
        },
    }
}

/// Lowers a primitive intrinsic with already-lowered Erlang arguments.
///
/// Inputs:
/// - `intrinsic`: compiler-owned primitive intrinsic identity.
/// - `args`: Erlang expressions already lowered from source or CoreIR
///   arguments.
///
/// Output:
/// - Erlang expression implementing the primitive intrinsic.
///
/// Transformation:
/// - Centralizes primitive runtime lowering so the transitional syntax bridge
///   and formal CoreIR emitter use the same BEAM implementation for primitive
///   receiver and module-call surfaces.
pub(super) fn lower_core_primitive_intrinsic_to_erlang(
    intrinsic: &CorePrimitiveIntrinsic,
    args: Vec<ErlExpr>,
) -> Option<ErlExpr> {
    match intrinsic {
        CorePrimitiveIntrinsic::BoolEqual => lower_core_bool_equal(args),
        CorePrimitiveIntrinsic::BoolCompare => lower_core_bool_compare(args),
        CorePrimitiveIntrinsic::BoolToString => lower_core_bool_to_string(args),
        CorePrimitiveIntrinsic::BoolFromString => lower_core_bool_from_string(args),
        CorePrimitiveIntrinsic::IntToString => lower_core_int_to_string(args),
        CorePrimitiveIntrinsic::IntFromString => lower_core_int_from_string(args),
        CorePrimitiveIntrinsic::FloatToString => lower_core_float_to_string(args),
        CorePrimitiveIntrinsic::FloatFromString => lower_core_float_from_string(args),
        CorePrimitiveIntrinsic::StringEqual => lower_core_string_equal(args),
        CorePrimitiveIntrinsic::StringCompare => lower_core_string_compare(args),
        CorePrimitiveIntrinsic::StringToString => lower_core_string_to_string(args),
        CorePrimitiveIntrinsic::StringFromString => lower_core_string_from_string(args),
        CorePrimitiveIntrinsic::StringIsEmpty => lower_core_string_is_empty(args),
        CorePrimitiveIntrinsic::StringAppend => lower_core_string_append(args),
        CorePrimitiveIntrinsic::StringConcat => lower_core_string_concat(args),
        CorePrimitiveIntrinsic::StringContains => lower_core_string_contains(args),
        CorePrimitiveIntrinsic::StringStartsWith => lower_core_string_starts_with(args),
        CorePrimitiveIntrinsic::StringEndsWith => lower_core_string_ends_with(args),
        CorePrimitiveIntrinsic::StringLength => lower_core_string_length(args),
        CorePrimitiveIntrinsic::StringByteSize => lower_core_string_byte_size(args),
        CorePrimitiveIntrinsic::StringLowercase => lower_core_string_unary_call("lowercase", args),
        CorePrimitiveIntrinsic::StringUppercase => lower_core_string_unary_call("uppercase", args),
        CorePrimitiveIntrinsic::StringTrim => lower_core_string_trim(args),
        CorePrimitiveIntrinsic::StringTrimStart => lower_core_string_trim_mode("leading", args),
        CorePrimitiveIntrinsic::StringTrimEnd => lower_core_string_trim_mode("trailing", args),
        CorePrimitiveIntrinsic::StringReplace => lower_core_string_replace(args),
        CorePrimitiveIntrinsic::StringSplit => lower_core_string_split(args),
        CorePrimitiveIntrinsic::StringSplitOnce => lower_core_string_split_once(args),
        CorePrimitiveIntrinsic::ListNew => lower_core_list_new(args),
        CorePrimitiveIntrinsic::ListIsEmpty => lower_core_list_is_empty(args),
        CorePrimitiveIntrinsic::ListLength => lower_core_list_length(args),
        CorePrimitiveIntrinsic::ListFirst => lower_core_list_first(args),
        CorePrimitiveIntrinsic::ListIterator => lower_core_list_iterator(args),
        CorePrimitiveIntrinsic::ListPush => lower_core_list_push(args),
        CorePrimitiveIntrinsic::ListClear => lower_core_list_clear(args),
        CorePrimitiveIntrinsic::IteratorNext => lower_core_iterator_next(args),
        CorePrimitiveIntrinsic::MapNew => lower_core_map_new(args),
        CorePrimitiveIntrinsic::MapIsEmpty => lower_core_map_is_empty(args),
        CorePrimitiveIntrinsic::MapSize => lower_core_map_size(args),
        CorePrimitiveIntrinsic::MapGet => lower_core_map_get(args),
        CorePrimitiveIntrinsic::MapContainsKey => lower_core_map_contains_key(args),
        CorePrimitiveIntrinsic::MapPut => lower_core_map_put(args),
        CorePrimitiveIntrinsic::MapRemove => lower_core_map_remove(args),
        CorePrimitiveIntrinsic::MapClear => lower_core_map_clear(args),
        CorePrimitiveIntrinsic::MapIterator => lower_core_map_iterator(args),
        CorePrimitiveIntrinsic::SetNew => lower_core_set_new(args),
        CorePrimitiveIntrinsic::SetIsEmpty => lower_core_set_is_empty(args),
        CorePrimitiveIntrinsic::SetSize => lower_core_set_size(args),
        CorePrimitiveIntrinsic::SetContains => lower_core_set_contains(args),
        CorePrimitiveIntrinsic::SetAdd => lower_core_set_add(args),
        CorePrimitiveIntrinsic::SetRemove => lower_core_set_remove(args),
        CorePrimitiveIntrinsic::SetClear => lower_core_set_clear(args),
        CorePrimitiveIntrinsic::SetIterator => lower_core_set_iterator(args),
    }
}

/// Lowers `runtime.console.println` to BEAM console output.
///
/// Inputs:
/// - `args`: one lowered Erlang text expression.
///
/// Output:
/// - `Some(begin io:format("~ts~n", [Text]), unit end)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Emits the target-owned BEAM `io:format/2` call behind the portable
///   `std.io.Console.println` API and normalizes the source-level return value
///   to Terlan `Unit`.
pub(super) fn lower_runtime_console_println(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [text] = exact_args(args, 1)?.try_into().ok()?;
    Some(ErlExpr::Raw(format!(
        "begin io:format(\"~ts~n\", [{}]), unit end",
        text.render()
    )))
}

/// Lowers `runtime.file.exists` to a BEAM regular-file check.
///
/// Inputs:
/// - `args`: one lowered Erlang path expression.
///
/// Output:
/// - `Some(filelib:is_regular(Path))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Emits a target-owned BEAM filesystem query behind the portable
///   `std.io.File.exists` API and returns Terlan's boolean representation.
pub(super) fn lower_runtime_file_exists(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path] = exact_args(args, 1)?.try_into().ok()?;
    Some(erl_remote_call("filelib", "is_regular", vec![path]))
}

/// Lowers `runtime.file.read_text` to BEAM file reading.
///
/// Inputs:
/// - `args`: one lowered Erlang path expression.
///
/// Output:
/// - `Some(case file:read_file(Path) of ... end)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Reads bytes through BEAM, decodes successful values as UTF-8 text, and
///   maps backend filesystem reasons into neutral `std.io.File.FileError`
///   atoms before returning the `Result[String, FileError]` shape.
pub(super) fn lower_runtime_file_read_text(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path] = exact_args(args, 1)?.try_into().ok()?;
    Some(ErlExpr::Raw(format!(
        "case file:read_file({}) of\n    {{ok, Bytes}} -> {{ok, unicode:characters_to_list(Bytes, utf8)}};\n    {{error, enoent}} -> {{error, not_found}};\n    {{error, eacces}} -> {{error, permission_denied}};\n    {{error, badarg}} -> {{error, invalid_path}};\n    {{error, _}} -> {{error, unknown}}\nend",
        path.render()
    )))
}

/// Lowers `runtime.file.write_text` to BEAM file writing.
///
/// Inputs:
/// - `args`: lowered Erlang path and text expressions.
///
/// Output:
/// - `Some(case file:write_file(Path, Text) of ... end)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Writes text through BEAM and maps backend filesystem reasons into neutral
///   `std.io.File.FileError` atoms before returning `Result[Unit, FileError]`.
pub(super) fn lower_runtime_file_write_text(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path, text] = exact_args(args, 2)?.try_into().ok()?;
    Some(ErlExpr::Raw(format!(
        "case file:write_file({}, {}) of\n    ok -> {{ok, unit}};\n    {{error, enoent}} -> {{error, not_found}};\n    {{error, eacces}} -> {{error, permission_denied}};\n    {{error, badarg}} -> {{error, invalid_path}};\n    {{error, _}} -> {{error, unknown}}\nend",
        path.render(),
        text.render()
    )))
}

/// Builds an Erlang remote function call expression.
///
/// Inputs:
/// - `module`: Terlan/CoreIR module name for the backend call.
/// - `function`: Erlang function name after any required sanitization.
/// - `args`: already-lowered Erlang argument expressions.
///
/// Output:
/// - Erlang remote-call expression.
///
/// Transformation:
/// - Stores the module/function/argument payload in the emitter AST and leaves
///   final module-name normalization to `ErlExpr::render`.
fn erl_remote_call(module: &str, function: &str, args: Vec<ErlExpr>) -> ErlExpr {
    ErlExpr::Call {
        module: Some(module.to_string()),
        function: function.to_string(),
        args,
    }
}

/// Builds an Erlang exact-equality expression.
///
/// Inputs:
/// - `left`: left Erlang expression.
/// - `right`: right Erlang expression.
///
/// Output:
/// - Erlang binary operation using `=:=`.
///
/// Transformation:
/// - Wraps the two lowered operands in the emitter AST with the exact equality
///   operator used by runtime string result checks.
fn erl_exact_eq(left: ErlExpr, right: ErlExpr) -> ErlExpr {
    ErlExpr::BinaryOp {
        op: ErlBinaryOp::EqEqEq,
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// Builds a Terlan option `some` value in the Erlang backend representation.
///
/// Inputs:
/// - `value`: Erlang expression payload.
///
/// Output:
/// - Erlang tuple expression representing `some(value)`.
///
/// Transformation:
/// - Uses a tagged tuple so optional CoreIR results have an explicit runtime
///   shape in the Erlang backend.
fn erl_some(value: ErlExpr) -> ErlExpr {
    ErlExpr::Tuple(vec![ErlExpr::Atom("some".to_string()), value])
}

/// Builds a Terlan option `none` value in the Erlang backend representation.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Erlang atom expression representing `none`.
///
/// Transformation:
/// - Uses the backend atom form chosen for CoreIR optional results.
fn erl_none() -> ErlExpr {
    ErlExpr::Atom("none".to_string())
}

/// Lowers `core.bool.equal` to Erlang exact equality.
///
/// Inputs:
/// - `args`: two lowered Erlang boolean expressions.
///
/// Output:
/// - `Some(left =:= right)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses BEAM exact equality as the implementation of the closed Terlan Bool
///   equality hook.
fn lower_core_bool_equal(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(erl_exact_eq(left, right))
}

/// Lowers `core.bool.compare` to the ordering comparison domain.
///
/// Inputs:
/// - `args`: two lowered Erlang boolean expressions.
///
/// Output:
/// - Erlang case expression returning `lt`, `eq`, or `gt`.
///
/// Transformation:
/// - Encodes Terlan's canonical `false < true` ordering behind the
///   backend-neutral Bool comparison intrinsic.
fn lower_core_bool_compare(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(ErlExpr::Tuple(vec![left, right])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("false".to_string()),
                    ErlPattern::Atom("true".to_string()),
                ]),
                guard: None,
                body: ErlExpr::Atom("lt".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("true".to_string()),
                    ErlPattern::Atom("false".to_string()),
                ]),
                guard: None,
                body: ErlExpr::Atom("gt".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: ErlExpr::Atom("eq".to_string()),
            },
        ],
    })
}

/// Lowers `core.bool.to_string` to canonical Bool text.
///
/// Inputs:
/// - `args`: one lowered Erlang boolean expression.
///
/// Output:
/// - Erlang case expression returning `"true"` or `"false"`.
///
/// Transformation:
/// - Converts the closed Terlan Bool runtime values into their canonical
///   source-level string spellings.
fn lower_core_bool_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(value),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Atom("true".to_string()),
                guard: None,
                body: ErlExpr::Binary("\"true\"".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Atom("false".to_string()),
                guard: None,
                body: ErlExpr::Binary("\"false\"".to_string()),
            },
        ],
    })
}

/// Lowers `core.bool.from_string` to a Terlan option shape.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(true)`, `some(false)`, or `none`.
///
/// Transformation:
/// - Recognizes only the canonical Bool strings admitted by
///   `std.core.String.Parse[Bool]`.
fn lower_core_bool_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(value),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Var("Value".to_string()),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Value".to_string()),
                    ErlExpr::Binary("\"true\"".to_string()),
                )),
                body: erl_some(ErlExpr::Atom("true".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Var("Value".to_string()),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Value".to_string()),
                    ErlExpr::Binary("\"false\"".to_string()),
                )),
                body: erl_some(ErlExpr::Atom("false".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.int.to_string` to Erlang integer display.
///
/// Inputs:
/// - `args`: one lowered Erlang integer expression.
///
/// Output:
/// - `Some(erlang:integer_to_list(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's integer rendering as the current BEAM implementation of the
///   Terlan canonical integer display contract.
fn lower_core_int_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call(
        "erlang",
        "integer_to_list",
        exact_args(args, 1)?,
    ))
}

/// Lowers `core.int.from_string` to Erlang integer parsing.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` only when parsing consumes
///   the whole string, otherwise `none`.
///
/// Transformation:
/// - Calls `string:to_integer/1`, checks for an empty rest string, and converts
///   Erlang parser output into the Terlan option runtime shape.
fn lower_core_int_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    lower_core_parse_from_string("to_integer", args)
}

/// Lowers `core.float.to_string` to Erlang finite-float display.
///
/// Inputs:
/// - `args`: one lowered Erlang float expression.
///
/// Output:
/// - `Some(erlang:float_to_list(value, Options))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses compact decimal formatting with sixteen decimals to preserve the
///   current 0.0.1 BEAM behavior behind a CoreIR intrinsic boundary.
fn lower_core_float_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "erlang",
        "float_to_list",
        vec![
            value,
            ErlExpr::List(vec![
                ErlExpr::Tuple(vec![
                    ErlExpr::Atom("decimals".to_string()),
                    ErlExpr::Int(16),
                ]),
                ErlExpr::Atom("compact".to_string()),
            ]),
        ],
    ))
}

/// Lowers `core.float.from_string` to Erlang float parsing.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` only when parsing consumes
///   the whole string, otherwise `none`.
///
/// Transformation:
/// - Calls `string:to_float/1`, checks for an empty rest string, and converts
///   Erlang parser output into the Terlan option runtime shape.
fn lower_core_float_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    lower_core_parse_from_string("to_float", args)
}

/// Lowers string-backed numeric parsing into a Terlan option expression.
///
/// Inputs:
/// - `function`: Erlang `string` module parser function name.
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` for a full parse and
///   `none` otherwise.
///
/// Transformation:
/// - Converts Erlang parser tuples `{Parsed, Rest}` into Terlan option values
///   and requires `Rest =:= ""` to avoid accepting prefixes.
fn lower_core_parse_from_string(function: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call("string", function, vec![value])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Var("Parsed".to_string()),
                    ErlPattern::Var("Rest".to_string()),
                ]),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Rest".to_string()),
                    ErlExpr::Binary("\"\"".to_string()),
                )),
                body: erl_some(ErlExpr::Var("Parsed".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.string.append` to Erlang string concatenation.
///
/// Inputs:
/// - `args`: two lowered Erlang string expressions.
///
/// Output:
/// - `Some(string:concat(left, right))` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Reuses Erlang's string concat primitive for the backend implementation of
///   Terlan string append.
fn lower_core_string_append(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "concat", exact_args(args, 2)?))
}

/// Lowers `core.string.equal` to Erlang exact equality.
///
/// Inputs:
/// - `args`: two lowered string expressions.
///
/// Output:
/// - `Some(left =:= right)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses exact equality for the current BEAM representation of Terlan UTF-8
///   strings while keeping the source operation backend neutral.
fn lower_core_string_equal(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(erl_exact_eq(left, right))
}

/// Lowers `core.string.compare` to the ordering comparison domain.
///
/// Inputs:
/// - `args`: two lowered string expressions.
///
/// Output:
/// - Erlang conditional returning `lt`, `eq`, or `gt`.
///
/// Transformation:
/// - Encodes Terlan's stable source string ordering behind the backend-neutral
///   String comparison intrinsic.
fn lower_core_string_compare(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(ErlExpr::If(vec![
        ErlIfClause {
            condition: erl_exact_eq(left.clone(), right.clone()),
            body: ErlExpr::Atom("eq".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::BinaryOp {
                op: ErlBinaryOp::Lt,
                left: Box::new(left),
                right: Box::new(right),
            },
            body: ErlExpr::Atom("lt".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::Atom("true".to_string()),
            body: ErlExpr::Atom("gt".to_string()),
        },
    ]))
}

/// Lowers `core.string.to_string` to an identity expression.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - The same Erlang expression when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Preserves the target string representation because `String` already is
///   its canonical textual form.
fn lower_core_string_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(value)
}

/// Lowers `core.string.from_string` to a successful Terlan option.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - `some(value)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Wraps the unchanged input because every Terlan `String` is already a
///   valid parsed `String` value.
fn lower_core_string_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_some(value))
}

/// Lowers `core.string.is_empty` to an empty-string comparison.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - `Some(value =:= "")` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Compares against the canonical empty string literal for the current BEAM
///   representation.
fn lower_core_string_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_exact_eq(value, ErlExpr::Binary("\"\"".to_string())))
}

/// Lowers `core.string.concat` to Erlang list append.
///
/// Inputs:
/// - `args`: one lowered Erlang list expression containing strings.
///
/// Output:
/// - `Some(lists:append(strings))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Maps collection concatenation to Erlang's list append operation.
fn lower_core_string_concat(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("lists", "append", exact_args(args, 1)?))
}

/// Lowers `core.string.contains` to a nomatch case expression.
///
/// Inputs:
/// - `args`: string value and search pattern expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Calls `string:find/2` and converts the Erlang `'nomatch'` sentinel into
///   a target-neutral boolean result.
fn lower_core_string_contains(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern] = exact_array_args(args)?;
    Some(nomatch_case_to_bool(erl_remote_call(
        "string",
        "find",
        vec![value, pattern],
    )))
}

/// Lowers `core.string.starts_with` to an Erlang prefix check.
///
/// Inputs:
/// - `args`: string value and prefix expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Calls `string:prefix/2` and converts the Erlang `'nomatch'` sentinel into
///   a target-neutral boolean result.
fn lower_core_string_starts_with(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern] = exact_array_args(args)?;
    Some(nomatch_case_to_bool(erl_remote_call(
        "string",
        "prefix",
        vec![value, pattern],
    )))
}

/// Lowers `core.string.ends_with` to an Erlang trailing search.
///
/// Inputs:
/// - `args`: string value and suffix expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Treats the empty suffix as always true, otherwise searches from the
///   trailing end and checks that the found suffix exactly equals the requested
///   suffix.
fn lower_core_string_ends_with(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, suffix] = exact_array_args(args)?;
    let search = erl_remote_call(
        "string",
        "find",
        vec![value, suffix.clone(), ErlExpr::Atom("trailing".to_string())],
    );
    Some(ErlExpr::If(vec![
        ErlIfClause {
            condition: erl_exact_eq(suffix.clone(), ErlExpr::Binary("\"\"".to_string())),
            body: ErlExpr::Atom("true".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::Atom("true".to_string()),
            body: ErlExpr::Case {
                scrutinee: Box::new(search),
                clauses: vec![
                    ErlCaseClause {
                        pattern: ErlPattern::Atom("nomatch".to_string()),
                        guard: None,
                        body: ErlExpr::Atom("false".to_string()),
                    },
                    ErlCaseClause {
                        pattern: ErlPattern::Var("Found".to_string()),
                        guard: None,
                        body: erl_exact_eq(ErlExpr::Var("Found".to_string()), suffix),
                    },
                ],
            },
        },
    ]))
}

/// Lowers `core.string.length` to Erlang string length.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:length(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Delegates user-visible character length to Erlang's unicode-aware string
///   length operation.
fn lower_core_string_length(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "length", exact_args(args, 1)?))
}

/// Lowers `core.string.byte_size` to Erlang UTF-8 byte-size logic.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(erlang:byte_size(unicode:characters_to_binary(value)))` when arity
///   is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Normalizes the string to a binary before measuring bytes so the backend
///   result matches the CoreIR UTF-8 byte-size contract.
fn lower_core_string_byte_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "erlang",
        "byte_size",
        vec![erl_remote_call(
            "unicode",
            "characters_to_binary",
            vec![value],
        )],
    ))
}

/// Lowers one-argument string intrinsics to Erlang `string:<function>/1`.
///
/// Inputs:
/// - `function`: Erlang string module function name.
/// - `args`: one lowered string expression.
///
/// Output:
/// - `Some(string:function(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Centralizes the backend mapping for lowercase and uppercase operations.
fn lower_core_string_unary_call(function: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", function, exact_args(args, 1)?))
}

/// Lowers `core.string.trim` to Erlang string trim.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:trim(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's default trim mode for the backend implementation.
fn lower_core_string_trim(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "trim", exact_args(args, 1)?))
}

/// Lowers directional string trim intrinsics to Erlang string trim modes.
///
/// Inputs:
/// - `mode`: Erlang trim mode atom name.
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:trim(value, mode))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Maps target-neutral trim-start and trim-end intrinsics to Erlang's
///   explicit `leading` and `trailing` mode atoms.
fn lower_core_string_trim_mode(mode: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "string",
        "trim",
        vec![value, ErlExpr::Atom(mode.to_string())],
    ))
}

/// Lowers `core.string.replace` to Erlang global string replacement.
///
/// Inputs:
/// - `args`: value, pattern, and replacement string expressions.
///
/// Output:
/// - Erlang expression flattening the result of `string:replace/4`.
///
/// Transformation:
/// - Calls Erlang with the `all` mode and flattens the iolist result so the
///   backend representation is a string value.
fn lower_core_string_replace(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern, replacement] = exact_array_args(args)?;
    Some(erl_remote_call(
        "lists",
        "flatten",
        vec![erl_remote_call(
            "string",
            "replace",
            vec![
                value,
                pattern,
                replacement,
                ErlExpr::Atom("all".to_string()),
            ],
        )],
    ))
}

/// Lowers `core.string.split` to Erlang global string splitting.
///
/// Inputs:
/// - `args`: value and separator string expressions.
///
/// Output:
/// - `Some(string:split(value, separator, all))` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's `all` split mode to implement the target-neutral list of
///   string fragments.
fn lower_core_string_split(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, separator] = exact_array_args(args)?;
    Some(erl_remote_call(
        "string",
        "split",
        vec![value, separator, ErlExpr::Atom("all".to_string())],
    ))
}

/// Lowers `core.string.split_once` to a Terlan option shape.
///
/// Inputs:
/// - `args`: value and separator string expressions.
///
/// Output:
/// - Erlang case expression returning `some({left, right})` or `none`.
///
/// Transformation:
/// - Calls Erlang's leading split operation and translates the result list into
///   the CoreIR option runtime shape for the backend.
fn lower_core_string_split_once(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, separator] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call(
            "string",
            "split",
            vec![value, separator, ErlExpr::Atom("leading".to_string())],
        )),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::List(vec![
                    ErlPattern::Var("Left".to_string()),
                    ErlPattern::Var("Right".to_string()),
                ]),
                guard: None,
                body: erl_some(ErlExpr::Tuple(vec![
                    ErlExpr::Var("Left".to_string()),
                    ErlExpr::Var("Right".to_string()),
                ])),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![ErlPattern::Wildcard]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.list.new` to an empty Erlang list.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty Erlang list expression.
///
/// Transformation:
/// - Hides the BEAM list representation behind the backend-neutral collection
///   intrinsic boundary.
fn lower_core_list_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::List(vec![]))
}

/// Lowers `core.list.is_empty` to an empty-list comparison.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Compares the receiver with the canonical empty Erlang list while keeping
///   Terlan source independent from that representation.
fn lower_core_list_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(erl_exact_eq(list, ErlExpr::List(vec![])))
}

/// Lowers `core.list.length` to `length/1`.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Integer Erlang expression for the number of list values.
///
/// Transformation:
/// - Delegates to the BEAM list runtime while preserving the portable Terlan
///   `List.length()` API.
fn lower_core_list_length(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(erl_remote_call("erlang", "length", vec![list]))
}

/// Lowers `core.list.first` to a Terlan `Option` shape.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - `{some, Head}` when the list is non-empty, otherwise `none`.
///
/// Transformation:
/// - Converts BEAM list pattern matching into Terlan's option runtime shape.
fn lower_core_list_first(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(list),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::ListCons(
                    Box::new(ErlPattern::Var("Head".to_string())),
                    Box::new(ErlPattern::Wildcard),
                ),
                guard: None,
                body: erl_some(ErlExpr::Var("Head".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.list.iterator` to the selected BEAM iterator state.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - The same Erlang list expression used as immutable traversal state.
///
/// Transformation:
/// - Starts portable traversal by reusing the BEAM list representation behind
///   the opaque `Iterator[T]` abstraction.
fn lower_core_list_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(list)
}

/// Lowers `core.iterator.next` to one immutable state-passing traversal step.
///
/// Inputs:
/// - `args`: one iterator state expression.
///
/// Output:
/// - Terlan option runtime shape: `none` for exhausted traversal, or
///   `{some, {CompilerValue, CompilerNextIterator}}` for one yielded value and
///   the next state.
///
/// Transformation:
/// - Pattern matches the backend iterator representation and returns the next
///   state explicitly instead of mutating the current iterator.
fn lower_core_iterator_next(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [iterator] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(iterator),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::ListCons(
                    Box::new(ErlPattern::Var("_TerlanIteratorValue".to_string())),
                    Box::new(ErlPattern::Var("_TerlanNextIterator".to_string())),
                ),
                guard: None,
                body: erl_some(ErlExpr::Tuple(vec![
                    ErlExpr::Var("_TerlanIteratorValue".to_string()),
                    ErlExpr::Var("_TerlanNextIterator".to_string()),
                ])),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.list.push` to an append-at-end list update.
///
/// Inputs:
/// - `args`: list and value expressions.
///
/// Output:
/// - Updated list expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
fn lower_core_list_push(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list, value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "lists",
        "append",
        vec![list, ErlExpr::List(vec![value])],
    ))
}

/// Lowers `core.list.clear` to an empty Erlang list.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Empty list expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
fn lower_core_list_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::List(vec![]))
}

/// Lowers `core.map.new` to an empty Erlang map.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty Erlang map expression.
///
/// Transformation:
/// - Hides the BEAM map representation behind the backend-neutral collection
///   intrinsic boundary.
fn lower_core_map_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.map.is_empty` to a BEAM map-size comparison.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Compares `maps:size(Map)` with zero without exposing that implementation
///   choice to Terlan source.
fn lower_core_map_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_exact_eq(
        erl_remote_call("maps", "size", vec![map]),
        ErlExpr::Int(0),
    ))
}

/// Lowers `core.map.size` to `maps:size/1`.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Integer Erlang expression for the number of key-value entries.
///
/// Transformation:
/// - Delegates to the BEAM map runtime while preserving the portable Terlan
///   `Map.size()` API.
fn lower_core_map_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "size", vec![map]))
}

/// Lowers `core.map.get` to a Terlan `Option` shape around `maps:find/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - `{some, Value}` when the key exists, otherwise `none`.
///
/// Transformation:
/// - Converts BEAM's `{ok, Value} | error` result into Terlan's option
///   runtime shape.
fn lower_core_map_get(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call("maps", "find", vec![key, map])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("ok".to_string()),
                    ErlPattern::Var("Value".to_string()),
                ]),
                guard: None,
                body: erl_some(ErlExpr::Var("Value".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Atom("error".to_string()),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.map.contains_key` to `maps:is_key/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Delegates key-presence checks to the BEAM map runtime.
fn lower_core_map_contains_key(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "is_key", vec![key, map]))
}

/// Lowers `core.map.put` to `maps:put/3`.
///
/// Inputs:
/// - `args`: map, key, and value expressions.
///
/// Output:
/// - Updated map expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
fn lower_core_map_put(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "put", vec![key, value, map]))
}

/// Lowers `core.map.remove` to `maps:remove/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - Updated map expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
fn lower_core_map_remove(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "remove", vec![key, map]))
}

/// Lowers `core.map.clear` to an empty Erlang map.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Empty map expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
fn lower_core_map_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.map.iterator` to the selected BEAM iterator state.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - A list of `{Key, Value}` tuples used as immutable traversal state.
///
/// Transformation:
/// - Converts the BEAM map backing shape to `maps:to_list(Map)` so the
///   existing portable iterator-next lowering can yield `{K, V}` entries.
fn lower_core_map_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "to_list", vec![map]))
}

/// Lowers `core.set.new` to the BEAM set backing shape.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty compiler-owned set expression.
///
/// Transformation:
/// - Represents the first BEAM set shape as an Erlang map from value to `true`
///   while preserving the backend-neutral Terlan `Set[T]` contract.
fn lower_core_set_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.set.is_empty` to a set-size comparison.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Observes the compiler-owned map-backed set shape without exposing it to
///   source code.
fn lower_core_set_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_exact_eq(
        erl_remote_call("maps", "size", vec![set]),
        ErlExpr::Int(0),
    ))
}

/// Lowers `core.set.size` to a map-size call over the backing shape.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Integer Erlang expression for the number of unique values.
///
/// Transformation:
/// - Uses the BEAM backing map size as the portable set cardinality.
fn lower_core_set_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "size", vec![set]))
}

/// Lowers `core.set.contains` to a key-presence check.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Treats set membership as key presence in the compiler-owned BEAM backing
///   shape.
fn lower_core_set_contains(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "is_key", vec![value, set]))
}

/// Lowers `core.set.add` to a map insertion into the backing shape.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Updated set expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
fn lower_core_set_add(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "maps",
        "put",
        vec![value, ErlExpr::Atom("true".to_string()), set],
    ))
}

/// Lowers `core.set.remove` to a map removal from the backing shape.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Updated set expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
fn lower_core_set_remove(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "remove", vec![value, set]))
}

/// Lowers `core.set.clear` to the empty BEAM set backing shape.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Empty compiler-owned set expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
fn lower_core_set_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.set.iterator` to the selected BEAM iterator state.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - A list of set values used as immutable traversal state.
///
/// Transformation:
/// - Converts the map-backed BEAM set shape to `maps:keys(Set)` so the
///   existing portable iterator-next lowering can yield `T` values.
fn lower_core_set_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "keys", vec![set]))
}

/// Converts Erlang string search sentinel results into booleans.
///
/// Inputs:
/// - `scrutinee`: Erlang expression returning either `'nomatch'` or a match
///   payload.
///
/// Output:
/// - Erlang case expression returning `false` for `'nomatch'` and `true`
///   otherwise.
///
/// Transformation:
/// - Hides Erlang's search sentinel behind Terlan's boolean intrinsic contract.
fn nomatch_case_to_bool(scrutinee: ErlExpr) -> ErlExpr {
    ErlExpr::Case {
        scrutinee: Box::new(scrutinee),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Atom("nomatch".to_string()),
                guard: None,
                body: ErlExpr::Atom("false".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: ErlExpr::Atom("true".to_string()),
            },
        ],
    }
}

/// Validates exact intrinsic arity for vector arguments.
///
/// Inputs:
/// - `args`: lowered Erlang argument expressions.
/// - `expected`: required arity.
///
/// Output:
/// - `Some(args)` when `args.len() == expected`.
/// - `None` when the intrinsic call has malformed arity.
///
/// Transformation:
/// - Performs arity validation without mutating or reordering arguments.
fn exact_args(args: Vec<ErlExpr>, expected: usize) -> Option<Vec<ErlExpr>> {
    (args.len() == expected).then_some(args)
}

/// Validates exact intrinsic arity and converts arguments into an array.
///
/// Inputs:
/// - `args`: lowered Erlang argument expressions.
///
/// Output:
/// - `Some([ErlExpr; N])` when the vector length matches `N`.
/// - `None` when the intrinsic call has malformed arity.
///
/// Transformation:
/// - Uses Rust's vector-to-array conversion so call sites can destructure
///   validated intrinsic arguments by position.
fn exact_array_args<const N: usize>(args: Vec<ErlExpr>) -> Option<[ErlExpr; N]> {
    args.try_into().ok()
}

/// Builds an Erlang body from a compiler intrinsic function annotation.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying annotations.
/// - `params`: function parameters used as intrinsic arguments.
///
/// Output:
/// - `Some(ErlExpr)` when `decl` has a supported `@compiler.intrinsic`
///   annotation and the intrinsic can lower for the Erlang backend.
/// - `None` when no supported intrinsic annotation is present.
///
/// Transformation:
/// - Parses the stable CoreIR intrinsic or runtime capability key from
///   annotation metadata, maps the function parameters to CoreIR variables,
///   builds a CoreIR intrinsic call, and delegates final target lowering to the
///   CoreIR intrinsic backend helper.
pub(super) fn lower_intrinsic_annotation_body(
    decl: &SyntaxDeclarationOutput,
    params: &[SyntaxParamOutput],
) -> Option<ErlExpr> {
    lower_intrinsic_annotation_body_for_names(decl, params.iter().map(|param| param.name.as_str()))
}

/// Builds an Erlang body from a compiler intrinsic annotation and argument names.
///
/// Inputs:
/// - `decl`: syntax-output declaration carrying annotations.
/// - `arg_names`: ordered source argument names passed to the intrinsic.
///
/// Output:
/// - `Some(ErlExpr)` when the declaration has a supported intrinsic annotation
///   and all arguments can be represented as Core variables.
/// - `None` when no supported annotation is present.
///
/// Transformation:
/// - Converts source argument names into CoreIR variables, builds a typed
///   CoreIR intrinsic call, and delegates the final lowering to the CoreIR
///   Erlang intrinsic backend.
pub(super) fn lower_intrinsic_annotation_body_for_names<'a>(
    decl: &SyntaxDeclarationOutput,
    arg_names: impl Iterator<Item = &'a str>,
) -> Option<ErlExpr> {
    let id = decl
        .annotations
        .iter()
        .find_map(core_intrinsic_id_from_annotation)?;
    let args = arg_names
        .map(|name| CoreExpr::Var(name.to_string()))
        .collect::<Vec<_>>();
    let call = CoreIntrinsicCall {
        return_type: core_intrinsic_return_type(&id),
        effects: core_intrinsic_effect_set(&id),
        id,
        args,
        span: decl.span.into(),
    };
    lower_core_intrinsic_call_to_erlang(&call)
}

/// Parses a supported CoreIR intrinsic key from an annotation.
///
/// Inputs:
/// - `annotation`: syntax-output annotation metadata.
///
/// Output:
/// - `Some(CoreIntrinsicId)` for supported `@compiler.intrinsic` keys.
/// - `None` for unrelated annotations or unsupported intrinsic keys.
///
/// Transformation:
/// - Requires the annotation path `compiler.intrinsic`, trims the preserved raw
///   metadata block, and maps the stable registry key into the backend's
///   compiler-owned intrinsic id.
fn core_intrinsic_id_from_annotation(
    annotation: &SyntaxAnnotationOutput,
) -> Option<CoreIntrinsicId> {
    if annotation.path != ["compiler", "intrinsic"] {
        return None;
    }
    let key = normalized_intrinsic_annotation_key(annotation.args.as_deref()?)?;
    core_intrinsic_id_from_key(key)
}

/// Normalizes preserved annotation metadata into an intrinsic key.
///
/// Inputs:
/// - `args`: raw annotation metadata text preserved by the parser.
///
/// Output:
/// - Trimmed intrinsic key text without the outer metadata braces.
///
/// Transformation:
/// - Accepts the parser's current `{...}` preservation shape and trims
///   surrounding whitespace so `@compiler.intrinsic {core.string.length}` maps
///   to `core.string.length`.
fn normalized_intrinsic_annotation_key(args: &str) -> Option<&str> {
    let args = args.trim();
    args.strip_prefix('{')?.strip_suffix('}').map(str::trim)
}

/// Maps a stable CoreIR intrinsic key into the compiler id.
///
/// Inputs:
/// - `key`: intrinsic registry key, such as `core.string.contains`.
///
/// Output:
/// - `Some(CoreIntrinsicId)` for the currently supported intrinsic set.
/// - `None` for unknown keys not handled by this backend.
///
/// Transformation:
/// - Converts documented registry strings into the closed Rust id consumed by
///   CoreIR and backend lowering.
fn core_intrinsic_id_from_key(key: &str) -> Option<CoreIntrinsicId> {
    match key {
        "core.int.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntToString,
        )),
        "core.int.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntFromString,
        )),
        "core.float.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatToString,
        )),
        "core.float.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatFromString,
        )),
        "core.string.equal" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringEqual,
        )),
        "core.string.compare" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringCompare,
        )),
        "core.string.to_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringToString,
        )),
        "core.string.from_string" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringFromString,
        )),
        "core.string.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringIsEmpty,
        )),
        "core.string.append" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringAppend,
        )),
        "core.string.concat" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringConcat,
        )),
        "core.string.contains" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringContains,
        )),
        "core.string.starts_with" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringStartsWith,
        )),
        "core.string.ends_with" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringEndsWith,
        )),
        "core.string.length" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringLength,
        )),
        "core.string.byte_size" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringByteSize,
        )),
        "core.string.lowercase" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringLowercase,
        )),
        "core.string.uppercase" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringUppercase,
        )),
        "core.string.trim" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrim,
        )),
        "core.string.trim_start" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrimStart,
        )),
        "core.string.trim_end" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrimEnd,
        )),
        "core.string.replace" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringReplace,
        )),
        "core.string.split" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringSplit,
        )),
        "core.string.split_once" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringSplitOnce,
        )),
        "core.list.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)),
        "core.list.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListIsEmpty,
        )),
        "core.list.length" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListLength,
        )),
        "core.list.first" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListFirst,
        )),
        "core.list.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListIterator,
        )),
        "core.list.push" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush)),
        "core.list.clear" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListClear,
        )),
        "core.iterator.next" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IteratorNext,
        )),
        "core.map.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew)),
        "core.map.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapIsEmpty,
        )),
        "core.map.size" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapSize)),
        "core.map.get" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet)),
        "core.map.contains_key" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapContainsKey,
        )),
        "core.map.put" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapPut)),
        "core.map.remove" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapRemove,
        )),
        "core.map.clear" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapClear)),
        "core.map.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapIterator,
        )),
        "core.set.new" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetNew)),
        "core.set.is_empty" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetIsEmpty,
        )),
        "core.set.size" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize)),
        "core.set.contains" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetContains,
        )),
        "core.set.add" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd)),
        "core.set.remove" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetRemove,
        )),
        "core.set.clear" => Some(CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetClear)),
        "core.set.iterator" => Some(CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetIterator,
        )),
        "runtime.console.println" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::ConsolePrintln,
        )),
        "runtime.file.exists" => Some(CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists)),
        "runtime.file.read_text" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileReadText,
        )),
        "runtime.file.write_text" => Some(CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::FileWriteText,
        )),
        _ => None,
    }
}

/// Returns the CoreIR return type for an intrinsic id.
///
/// Inputs:
/// - `id`: compiler-owned intrinsic identity.
///
/// Output:
/// - Backend-neutral CoreIR return type for the intrinsic or runtime
///   capability.
///
/// Transformation:
/// - Mirrors the documented intrinsic and runtime capability registries so
///   annotation-driven backend emission can construct a typed CoreIR intrinsic
///   call without re-reading source function signatures.
fn core_intrinsic_return_type(id: &CoreIntrinsicId) -> CoreType {
    match id {
        CoreIntrinsicId::Primitive(intrinsic) => core_primitive_intrinsic_return_type(intrinsic),
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::ConsolePrintln) => {
            CoreType::Named("Unit".to_string())
        }
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileExists) => CoreType::Bool,
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileReadText) => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::String,
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
        CoreIntrinsicId::Runtime(CoreRuntimeCapability::FileWriteText) => CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Unit".to_string()),
                CoreType::Named("std.io.File.FileError".to_string()),
            ],
        },
    }
}

/// Returns the CoreIR effect set for an intrinsic id.
///
/// Inputs:
/// - `id`: compiler-owned intrinsic identity.
///
/// Output:
/// - Backend-neutral effect set attached to the intrinsic call.
///
/// Transformation:
/// - Marks primitive intrinsics as pure and runtime console output as `io` so
///   downstream CoreIR consumers can distinguish value computations from
///   observable effects.
fn core_intrinsic_effect_set(id: &CoreIntrinsicId) -> CoreEffectSet {
    match id {
        CoreIntrinsicId::Primitive(_) => CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        CoreIntrinsicId::Runtime(
            CoreRuntimeCapability::ConsolePrintln
            | CoreRuntimeCapability::FileExists
            | CoreRuntimeCapability::FileReadText
            | CoreRuntimeCapability::FileWriteText,
        ) => CoreEffectSet {
            effects: vec!["io".to_string()],
        },
    }
}

/// Returns the CoreIR return type for a primitive intrinsic key.
///
/// Inputs:
/// - `intrinsic`: compiler-owned primitive intrinsic identity.
///
/// Output:
/// - Backend-neutral CoreIR return type for the intrinsic.
///
/// Transformation:
/// - Mirrors the documented primitive intrinsic registry so annotation-driven
///   backend emission can construct a typed CoreIR intrinsic call without
///   re-reading source function signatures.
fn core_primitive_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::BoolToString
        | CorePrimitiveIntrinsic::IntToString
        | CorePrimitiveIntrinsic::FloatToString
        | CorePrimitiveIntrinsic::StringToString
        | CorePrimitiveIntrinsic::StringAppend
        | CorePrimitiveIntrinsic::StringConcat
        | CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringUppercase
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringReplace => CoreType::String,
        CorePrimitiveIntrinsic::BoolEqual => CoreType::Bool,
        CorePrimitiveIntrinsic::BoolCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::StringCompare => {
            CoreType::Named("std.core.Ordering.Comparison".to_string())
        }
        CorePrimitiveIntrinsic::BoolFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Bool],
        },
        CorePrimitiveIntrinsic::StringFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::String],
        },
        CorePrimitiveIntrinsic::IntFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Int],
        },
        CorePrimitiveIntrinsic::FloatFromString => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Float],
        },
        CorePrimitiveIntrinsic::StringEqual
        | CorePrimitiveIntrinsic::StringIsEmpty
        | CorePrimitiveIntrinsic::StringContains
        | CorePrimitiveIntrinsic::StringStartsWith
        | CorePrimitiveIntrinsic::StringEndsWith => CoreType::Bool,
        CorePrimitiveIntrinsic::StringLength | CorePrimitiveIntrinsic::StringByteSize => {
            CoreType::Int
        }
        CorePrimitiveIntrinsic::StringSplit => CoreType::List(Box::new(CoreType::String)),
        CorePrimitiveIntrinsic::StringSplitOnce => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::String),
                CoreTupleTypeElem::Type(CoreType::String),
            ])],
        },
        CorePrimitiveIntrinsic::ListNew
        | CorePrimitiveIntrinsic::ListIterator
        | CorePrimitiveIntrinsic::ListPush
        | CorePrimitiveIntrinsic::ListClear => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::ListIsEmpty => CoreType::Bool,
        CorePrimitiveIntrinsic::ListLength => CoreType::Int,
        CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::IteratorNext => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::MapNew
        | CorePrimitiveIntrinsic::MapPut
        | CorePrimitiveIntrinsic::MapRemove
        | CorePrimitiveIntrinsic::MapClear => CoreType::Named("Map".to_string()),
        CorePrimitiveIntrinsic::MapIterator => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::MapIsEmpty | CorePrimitiveIntrinsic::MapContainsKey => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::MapSize => CoreType::Int,
        CorePrimitiveIntrinsic::MapGet => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Named("Dynamic".to_string())],
        },
        CorePrimitiveIntrinsic::SetNew
        | CorePrimitiveIntrinsic::SetAdd
        | CorePrimitiveIntrinsic::SetRemove
        | CorePrimitiveIntrinsic::SetClear => CoreType::Named("Set".to_string()),
        CorePrimitiveIntrinsic::SetIterator => {
            CoreType::List(Box::new(CoreType::Named("Dynamic".to_string())))
        }
        CorePrimitiveIntrinsic::SetIsEmpty | CorePrimitiveIntrinsic::SetContains => CoreType::Bool,
        CorePrimitiveIntrinsic::SetSize => CoreType::Int,
    }
}
