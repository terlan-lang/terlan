use super::*;

/// Lowers Terlan type-introspection calls through backend-private type values.
///
/// Inputs:
/// - `function`: local or `std.core.Type` function name.
/// - `args`: original syntax-output call arguments.
/// - `env`: lexical environment with known local value types.
///
/// Output:
/// - Erlang expression for `type_of(value)` or `is_type(value, Type)`.
/// - `None` when the function or argument shape is not part of the supported
///   type-introspection subset.
///
/// Transformation:
/// - Classifies the original syntax expression before ordinary argument
///   lowering so implicit type names such as `Int` remain type values instead
///   of becoming Erlang variables.
pub(super) fn lower_syntax_type_intrinsic_call(
    function: &str,
    args: &[SyntaxExprOutput],
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match (function, args) {
        ("type_of", [value]) => lower_syntax_static_type_value(value, env),
        ("is_type", [value, expected]) => Some(syntax_erl_exact_eq(
            lower_syntax_static_type_value(value, env)?,
            lower_syntax_type_value_expr(expected, env)?,
        )),
        _ => None,
    }
}

/// Returns an internal BEAM expression for a static syntax-output type value.
///
/// Inputs:
/// - `expr`: source expression whose type should be represented.
/// - `env`: lexical environment with value-type annotations for locals.
///
/// Output:
/// - Erlang atom used internally for supported Terlan type values.
/// - `None` when the expression cannot be classified statically.
///
/// Transformation:
/// - Converts syntax-output expression shape into a backend-private atom. These
///   atoms are compiler implementation details, not user-visible atom literals.
fn lower_syntax_static_type_value(
    expr: &SyntaxExprOutput,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Int => Some(syntax_erlang_type_value_atom("int")),
        SyntaxExprKind::Float => Some(syntax_erlang_type_value_atom("float")),
        SyntaxExprKind::Binary => Some(syntax_erlang_type_value_atom("string")),
        SyntaxExprKind::Atom => match expr.text.as_deref()? {
            "true" | "false" => Some(syntax_erlang_type_value_atom("bool")),
            _ => Some(syntax_erlang_type_value_atom("atom")),
        },
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            if name == "Unit" {
                return Some(syntax_erlang_type_value_atom("unit"));
            }
            if is_bool_literal_name(name) {
                return Some(syntax_erlang_type_value_atom("bool"));
            }
            if is_syntax_type_value_name(name) {
                return Some(syntax_erlang_type_value_atom("type"));
            }
            env.value_types
                .get(name)
                .and_then(|type_text| syntax_erlang_type_value_from_type_text(type_text))
        }
        SyntaxExprKind::Tuple => Some(syntax_erlang_type_value_atom("tuple")),
        SyntaxExprKind::List | SyntaxExprKind::ListCons | SyntaxExprKind::ListComprehension => {
            Some(syntax_erlang_type_value_atom("list"))
        }
        SyntaxExprKind::FixedArray => Some(syntax_erlang_type_value_atom("fixed_array")),
        SyntaxExprKind::Map => Some(syntax_erlang_type_value_atom("map")),
        SyntaxExprKind::RecordConstruct | SyntaxExprKind::RecordUpdate => {
            Some(syntax_erlang_type_value_atom("record"))
        }
        SyntaxExprKind::Call if expr.text.as_deref() == Some("type_of") => {
            Some(syntax_erlang_type_value_atom("type"))
        }
        _ => None,
    }
}

/// Lowers an expression expected to already be a Terlan type value.
///
/// Inputs:
/// - `expr`: source expression used as the expected type in `is_type`.
/// - `env`: lexical environment used for fallback static classification.
///
/// Output:
/// - Internal BEAM type-value atom.
///
/// Transformation:
/// - Recognizes implicit source type names represented as syntax variables and
///   otherwise falls back to classifying ordinary expressions.
fn lower_syntax_type_value_expr(expr: &SyntaxExprOutput, env: &SyntaxLowerEnv) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Var if is_syntax_type_value_name(expr.text.as_deref()?) => Some(
            syntax_erlang_type_value_atom(&expr.text.as_deref()?.to_ascii_lowercase()),
        ),
        _ => lower_syntax_static_type_value(expr, env),
    }
}

/// Checks whether a syntax variable name denotes an implicit type value.
///
/// Inputs:
/// - `name`: source variable/name reference text.
///
/// Output:
/// - `true` when the name belongs to the implicit type-value prelude.
///
/// Transformation:
/// - Mirrors the current compiler-backed type-introspection prelude.
fn is_syntax_type_value_name(name: &str) -> bool {
    matches!(
        name,
        "Unit" | "Bool" | "Int" | "Float" | "String" | "Atom" | "Type"
    )
}

/// Converts a source type annotation into an internal type-value atom.
///
/// Inputs:
/// - `type_text`: source type annotation text, possibly qualified or generic.
///
/// Output:
/// - Backend-private type atom for supported intrinsic type heads.
/// - `None` for user-defined or not-yet-classified type heads.
///
/// Transformation:
/// - Reuses the receiver type-head normalization to strip qualification and
///   generic arguments, then maps recognized Terlan core type names.
fn syntax_erlang_type_value_from_type_text(type_text: &str) -> Option<ErlExpr> {
    match receiver_type_head(type_text).as_str() {
        "Unit" => Some(syntax_erlang_type_value_atom("unit")),
        "Bool" => Some(syntax_erlang_type_value_atom("bool")),
        "Int" => Some(syntax_erlang_type_value_atom("int")),
        "Float" => Some(syntax_erlang_type_value_atom("float")),
        "String" => Some(syntax_erlang_type_value_atom("string")),
        "Atom" => Some(syntax_erlang_type_value_atom("atom")),
        "Type" => Some(syntax_erlang_type_value_atom("type")),
        "List" => Some(syntax_erlang_type_value_atom("list")),
        "Map" => Some(syntax_erlang_type_value_atom("map")),
        "Set" => Some(syntax_erlang_type_value_atom("set")),
        _ => None,
    }
}

/// Builds a backend-private Erlang atom for a Terlan type value.
///
/// Inputs:
/// - `name`: lowercase type-value payload.
///
/// Output:
/// - Erlang atom namespaced under `terlan_type_`.
///
/// Transformation:
/// - Keeps compiler type values separate from source atom literals and backend
///   runtime atoms.
fn syntax_erlang_type_value_atom(name: &str) -> ErlExpr {
    ErlExpr::Atom(format!("terlan_type_{name}"))
}

/// Builds an Erlang exact-equality expression for syntax bridge intrinsics.
///
/// Inputs:
/// - `left`: left Erlang expression.
/// - `right`: right Erlang expression.
///
/// Output:
/// - Erlang binary operation using `=:=`.
///
/// Transformation:
/// - Wraps two internal type-value operands in the emitter AST with exact
///   equality so `is_type` is deterministic across targets.
fn syntax_erl_exact_eq(left: ErlExpr, right: ErlExpr) -> ErlExpr {
    ErlExpr::BinaryOp {
        op: ErlBinaryOp::EqEqEq,
        left: Box::new(left),
        right: Box::new(right),
    }
}
