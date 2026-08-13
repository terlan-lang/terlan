//! Concrete boundary-type recovery for compiler-generated `Dynamic` wrappers.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CorePattern, CoreTupleTypeElem, CoreType};

/// Recovers one concrete return type from a compiler-generated dynamic entry.
///
/// Recovery is structural and fail-closed. It handles only expression forms
/// whose complete type is recoverable from checked CoreIR without name
/// resolution or a second typechecker.
pub(super) fn inferred_dynamic_return_type(function: &CoreFunction) -> Option<CoreType> {
    if !function.core_return_type.as_ref().is_some_and(|ty| {
        matches!(ty, CoreType::Dynamic) || matches!(ty, CoreType::Named(name) if name == "Dynamic")
    }) {
        return None;
    }
    let variables = function
        .params
        .iter()
        .map(|parameter| {
            parameter
                .core_ty
                .clone()
                .map(|ty| (parameter.name.clone(), ty))
        })
        .collect::<Option<HashMap<_, _>>>()?;
    let body = function.clauses.first()?.body.core_expr.as_ref()?;
    infer_expression_type(body, &variables)
}

/// Infers one unambiguous structural CoreIR expression type.
pub(super) fn infer_expression_type(
    expression: &CoreExpr,
    variables: &HashMap<String, CoreType>,
) -> Option<CoreType> {
    match expression {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "Unit" => {
            Some(CoreType::Named("Unit".to_string()))
        }
        CoreExpr::Atom(value) | CoreExpr::Var(value)
            if matches!(value.as_str(), "true" | "false") =>
        {
            Some(CoreType::Bool)
        }
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| infer_expression_type(item, variables).map(CoreTupleTypeElem::Type))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::List(items) => infer_homogeneous_list(items, variables),
        CoreExpr::Let { bindings, body } => {
            let mut scoped = variables.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let inferred = infer_expression_type(&binding.value, &scoped);
                // Core sequencing is represented as generated let bindings.
                // An effectful intermediate expression need not have a
                // structurally recoverable result type when the final value
                // does not depend on it. Remove any shadowed value first so a
                // later reference still fails closed instead of inheriting a
                // stale outer type.
                scoped.remove(name);
                if let Some(ty) = inferred {
                    scoped.insert(name.clone(), ty);
                }
            }
            infer_expression_type(body, &scoped)
        }
        CoreExpr::If { clauses } => common_type(
            clauses
                .iter()
                .map(|clause| infer_expression_type(&clause.body, variables)),
        ),
        CoreExpr::UnaryOp { operator, operand } => {
            let operand = infer_expression_type(operand, variables)?;
            match operator.as_str() {
                "-" if matches!(operand, CoreType::Int | CoreType::Float) => Some(operand),
                "not" | "!" if operand == CoreType::Bool => Some(CoreType::Bool),
                _ => None,
            }
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => infer_binary_type(
            operator,
            infer_expression_type(left, variables)?,
            infer_expression_type(right, variables)?,
        ),
        CoreExpr::Cast { expr, target_type }
            if matches!(target_type, CoreType::Dynamic)
                || matches!(target_type, CoreType::Named(name) if name == "Dynamic") =>
        {
            infer_expression_type(expr, variables)
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        _ => None,
    }
}

/// Infers a nonempty list only when every item has one identical type.
fn infer_homogeneous_list(
    items: &[CoreExpr],
    variables: &HashMap<String, CoreType>,
) -> Option<CoreType> {
    let item = common_type(
        items
            .iter()
            .map(|item| infer_expression_type(item, variables)),
    )?;
    Some(CoreType::List(Box::new(item)))
}

/// Returns the one type shared by a nonempty sequence.
fn common_type(types: impl IntoIterator<Item = Option<CoreType>>) -> Option<CoreType> {
    let mut types = types.into_iter();
    let first = types.next()??;
    types.all(|ty| ty.as_ref() == Some(&first)).then_some(first)
}

/// Infers the result of one closed scalar binary operation.
fn infer_binary_type(operator: &str, left: CoreType, right: CoreType) -> Option<CoreType> {
    let numeric = matches!(left, CoreType::Int | CoreType::Float)
        && matches!(right, CoreType::Int | CoreType::Float);
    match operator {
        "+" | "-" | "*" | "/" if numeric => {
            Some(if left == CoreType::Float || right == CoreType::Float {
                CoreType::Float
            } else {
                CoreType::Int
            })
        }
        "div" | "rem" if left == CoreType::Int && right == CoreType::Int => Some(CoreType::Int),
        "==" | "!=" if left == right => Some(CoreType::Bool),
        "<" | "<=" | ">" | ">=" if numeric => Some(CoreType::Bool),
        "and" | "or" if left == CoreType::Bool && right == CoreType::Bool => Some(CoreType::Bool),
        _ => None,
    }
}
