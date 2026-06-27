use crate::terlan_typeck::{CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::super::erl::*;
use super::erl_exact_eq;

/// Lowers `core.type.type_of` from original CoreIR arguments.
///
/// Inputs:
/// - `args`: original CoreIR intrinsic arguments.
///
/// Output:
/// - Erlang expression representing the static Terlan type value.
/// - `None` when the argument shape does not have a static CoreIR type value
///   in the current backend subset.
///
/// Transformation:
/// - Classifies the CoreIR expression directly instead of using BEAM runtime
///   reflection, keeping the source feature compiler-owned and target-neutral.
pub(super) fn lower_core_type_of(args: &[CoreExpr]) -> Option<ErlExpr> {
    let [value] = args else {
        return None;
    };
    lower_core_static_type_value(value)
}

/// Lowers `core.type.is_type` from original CoreIR arguments.
///
/// Inputs:
/// - `args`: original CoreIR intrinsic arguments.
///
/// Output:
/// - Erlang equality expression comparing two internal type values.
/// - `None` when either side cannot be represented as a static type value.
///
/// Transformation:
/// - Computes `type_of(value) == Type` at compile-lowered expression level,
///   avoiding backend-specific runtime type checks.
pub(super) fn lower_core_is_type(args: &[CoreExpr]) -> Option<ErlExpr> {
    let [value, expected] = args else {
        return None;
    };
    Some(erl_exact_eq(
        lower_core_static_type_value(value)?,
        lower_core_type_value_expr(expected)?,
    ))
}

/// Handles unreachable lowered-argument `type_of` dispatch.
///
/// Inputs:
/// - `args`: lowered Erlang arguments from a primitive intrinsic dispatch.
///
/// Output:
/// - Always `None`.
///
/// Transformation:
/// - Documents that `type_of` requires original CoreIR argument inspection and
///   is handled before generic argument lowering.
pub(super) fn lower_core_type_of_intrinsic(_args: Vec<ErlExpr>) -> Option<ErlExpr> {
    None
}

/// Handles unreachable lowered-argument `is_type` dispatch.
///
/// Inputs:
/// - `args`: lowered Erlang arguments from a primitive intrinsic dispatch.
///
/// Output:
/// - Always `None`.
///
/// Transformation:
/// - Documents that `is_type` requires original CoreIR argument inspection and
///   is handled before generic argument lowering.
pub(super) fn lower_core_is_type_intrinsic(_args: Vec<ErlExpr>) -> Option<ErlExpr> {
    None
}

/// Returns an internal BEAM expression for a static CoreIR type value.
///
/// Inputs:
/// - `expr`: CoreIR expression whose type should be represented.
///
/// Output:
/// - Erlang atom used internally for supported Terlan type values.
/// - `None` when the type cannot be classified from expression shape alone.
///
/// Transformation:
/// - Converts CoreIR expression shape into a backend-private atom. These atoms
///   are implementation details and are not Terlan source atom literals.
fn lower_core_static_type_value(expr: &CoreExpr) -> Option<ErlExpr> {
    match expr {
        CoreExpr::Int(_) => Some(erlang_type_value_atom("int")),
        CoreExpr::Float(_) => Some(erlang_type_value_atom("float")),
        CoreExpr::Binary(_) => Some(erlang_type_value_atom("string")),
        CoreExpr::Atom(value) if value == "true" || value == "false" => {
            Some(erlang_type_value_atom("bool"))
        }
        CoreExpr::Atom(_) => Some(erlang_type_value_atom("atom")),
        CoreExpr::Var(name) if name == "Unit" => Some(erlang_type_value_atom("unit")),
        CoreExpr::Var(name) if name == "true" || name == "false" => {
            Some(erlang_type_value_atom("bool"))
        }
        CoreExpr::Var(name) if is_core_type_value_name(name) => {
            Some(erlang_type_value_atom("type"))
        }
        CoreExpr::Tuple(_) => Some(erlang_type_value_atom("tuple")),
        CoreExpr::List(_) | CoreExpr::ListCons { .. } => Some(erlang_type_value_atom("list")),
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::TypeOf)
            ) =>
        {
            Some(erlang_type_value_atom("type"))
        }
        _ => None,
    }
}

/// Lowers an expression that is expected to already be a type value.
///
/// Inputs:
/// - `expr`: CoreIR expression used as the expected type in `is_type`.
///
/// Output:
/// - Internal BEAM type-value atom.
///
/// Transformation:
/// - Recognizes implicit source type names represented as CoreIR variables and
///   falls back to static expression classification for nested type-producing
///   expressions.
fn lower_core_type_value_expr(expr: &CoreExpr) -> Option<ErlExpr> {
    match expr {
        CoreExpr::Var(name) if is_core_type_value_name(name) => {
            Some(erlang_type_value_atom(&name.to_ascii_lowercase()))
        }
        _ => lower_core_static_type_value(expr),
    }
}

/// Checks whether a CoreIR variable name denotes an implicit type value.
///
/// Inputs:
/// - `name`: CoreIR variable name.
///
/// Output:
/// - `true` when the name belongs to the implicit type-value prelude.
///
/// Transformation:
/// - Mirrors the compiler's minimal implicit type-value set for BEAM lowering.
fn is_core_type_value_name(name: &str) -> bool {
    matches!(
        name,
        "Unit" | "Bool" | "Int" | "Float" | "String" | "Atom" | "Type"
    )
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
/// - Keeps compiler type values separate from user-visible atom literals and
///   backend runtime atoms.
fn erlang_type_value_atom(name: &str) -> ErlExpr {
    ErlExpr::Atom(format!("terlan_type_{name}"))
}
