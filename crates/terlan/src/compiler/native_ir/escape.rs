//! Conservative managed-constructor escape analysis.

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::free_variables;

/// Selects let bindings that must still execute after dead allocation removal.
pub(super) fn retained_managed_bindings(bindings: &[CoreLetBinding], body: &CoreExpr) -> Vec<bool> {
    let mut live = free_variables(body);
    let mut retained = vec![true; bindings.len()];
    for (index, binding) in bindings.iter().enumerate().rev() {
        let CorePattern::Var(name) = &binding.pattern else {
            live.extend(free_variables(&binding.value));
            continue;
        };
        let result_is_live = live.remove(name);
        if !result_is_live && is_allocation_only_constructor(&binding.value) {
            retained[index] = false;
            continue;
        }
        live.extend(free_variables(&binding.value));
    }
    retained
}

/// Reports whether evaluating an expression only computes immutable native values.
fn is_allocation_only_value(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Int(_) | CoreExpr::Float(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => true,
        CoreExpr::ConstructorCall { args, .. } => args.iter().all(is_allocation_only_value),
        CoreExpr::UnaryOp { operand, .. } => is_allocation_only_value(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            is_allocation_only_value(left) && is_allocation_only_value(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                matches!(binding.pattern, CorePattern::Var(_))
                    && is_allocation_only_value(&binding.value)
            }) && is_allocation_only_value(body)
        }
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses.iter().all(|clause| {
                    is_allocation_only_value(&clause.condition)
                        && is_allocation_only_value(&clause.body)
                })
        }
        _ => false,
    }
}

/// Reports whether a dead expression is exactly an effect-free constructor graph.
fn is_allocation_only_constructor(expr: &CoreExpr) -> bool {
    matches!(expr, CoreExpr::ConstructorCall { args, .. } if args.iter().all(is_allocation_only_value))
}
