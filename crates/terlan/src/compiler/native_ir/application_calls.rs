//! Closed-application call locality checks for direct AOT admission.

use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::{condition_yield_region_at_depth, expr_is_scalar, YieldRegion};

/// Extracts the first suspending argument while preserving eager prefix order.
pub(super) fn eager_argument_yield(
    args: &[CoreExpr],
    depth: usize,
) -> Option<(YieldRegion, Vec<CoreExpr>)> {
    for (yield_index, arg) in args.iter().enumerate() {
        let Some(mut region) = condition_yield_region_at_depth(arg, depth.saturating_add(1)) else {
            if !expr_is_scalar(arg) {
                return None;
            }
            continue;
        };
        let mut resumed = args.to_vec();
        let mut prefix = Vec::with_capacity(yield_index + region.prefix.len());
        for (index, earlier) in args[..yield_index].iter().enumerate() {
            let name = format!("$native_eager_arg_{depth}_{index}");
            prefix.push(CoreLetBinding {
                pattern: CorePattern::Var(name.clone()),
                value: earlier.clone(),
            });
            resumed[index] = CoreExpr::Var(name);
        }
        prefix.append(&mut region.prefix);
        resumed[yield_index] = region.resume.clone();
        region.prefix = prefix;
        return Some((region, resumed));
    }
    None
}

pub(super) fn expr_calls_are_local(expr: &CoreExpr, identities: &[(&str, usize)]) -> bool {
    match expr {
        CoreExpr::Call { function, args } => {
            identities
                .iter()
                .any(|(name, arity)| *name == function && *arity == args.len())
                && args.iter().all(|arg| expr_calls_are_local(arg, identities))
        }
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().all(|arg| expr_calls_are_local(arg, identities))
        }
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .all(|field| expr_calls_are_local(&field.value, identities)),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_calls_are_local(base, identities)
                && fields
                    .iter()
                    .all(|field| expr_calls_are_local(&field.value, identities))
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            expr_calls_are_local(operand, identities)
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_are_local(base, identities)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_calls_are_local(left, identities) && expr_calls_are_local(right, identities)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .all(|binding| expr_calls_are_local(&binding.value, identities))
                && expr_calls_are_local(body, identities)
        }
        CoreExpr::If { clauses } => clauses.iter().all(|clause| {
            expr_calls_are_local(&clause.condition, identities)
                && expr_calls_are_local(&clause.body, identities)
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            expr_calls_are_local(scrutinee, identities)
                && clauses.iter().all(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_none_or(|guard| expr_calls_are_local(guard, identities))
                        && expr_calls_are_local(&clause.body, identities)
                })
        }
        _ => true,
    }
}

/// Reports whether one expression reaches a known suspending function.
pub(super) fn expr_calls_suspending(
    expr: &CoreExpr,
    suspending: &HashSet<(String, usize)>,
) -> bool {
    match expr {
        CoreExpr::Call { function, args } => {
            suspending.contains(&(function.clone(), args.len()))
                || args
                    .iter()
                    .any(|arg| expr_calls_suspending(arg, suspending))
        }
        CoreExpr::FunctionCall { .. } => true,
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .any(|arg| expr_calls_suspending(arg, suspending)),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .any(|field| expr_calls_suspending(&field.value, suspending)),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_calls_suspending(base, suspending)
                || fields
                    .iter()
                    .any(|field| expr_calls_suspending(&field.value, suspending))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_suspending(base, suspending)
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            expr_calls_suspending(operand, suspending)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_calls_suspending(left, suspending) || expr_calls_suspending(right, suspending)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expr_calls_suspending(&binding.value, suspending))
                || expr_calls_suspending(body, suspending)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expr_calls_suspending(&clause.condition, suspending)
                || expr_calls_suspending(&clause.body, suspending)
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            expr_calls_suspending(scrutinee, suspending)
                || clauses.iter().any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_suspending(guard, suspending))
                        || expr_calls_suspending(&clause.body, suspending)
                })
        }
        _ => false,
    }
}
