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
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().all(|arg| expr_calls_are_local(arg, identities))
        }
        CoreExpr::FunctionCall { callee, args } => {
            expr_calls_are_local(callee, identities)
                && args.iter().all(|arg| expr_calls_are_local(arg, identities))
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter()
            .all(|item| expr_calls_are_local(item, identities)),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => expr_calls_are_local(head, identities) && expr_calls_are_local(tail, identities),
        CoreExpr::Map(fields) => fields
            .iter()
            .all(|field| expr_calls_are_local(&field.value, identities)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .all(|field| expr_calls_are_local(&field.value, identities))
        }
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
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().all(|arg| expr_calls_are_local(arg, identities))
                && expr_calls_are_local(record, identities)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            expr_calls_are_local(receiver, identities)
                && args.iter().all(|arg| expr_calls_are_local(arg, identities))
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            expr_calls_are_local(expr, identities)
                && generators
                    .iter()
                    .all(|generator| expr_calls_are_local(&generator.source, identities))
                && guards
                    .iter()
                    .all(|guard| expr_calls_are_local(guard, identities))
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
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            expr_calls_are_local(body, identities)
                && of_clauses.iter().chain(catch_clauses).all(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_none_or(|guard| expr_calls_are_local(guard, identities))
                        && expr_calls_are_local(&clause.body, identities)
                })
                && after_clause.as_ref().is_none_or(|after| {
                    expr_calls_are_local(&after.trigger, identities)
                        && expr_calls_are_local(&after.body, identities)
                })
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .all(|parameter| expr_calls_are_local(parameter, identities)),
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
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .any(|arg| expr_calls_suspending(arg, suspending)),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter()
            .any(|item| expr_calls_suspending(item, suspending)),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => expr_calls_suspending(head, suspending) || expr_calls_suspending(tail, suspending),
        CoreExpr::Map(fields) => fields
            .iter()
            .any(|field| expr_calls_suspending(&field.value, suspending)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .any(|field| expr_calls_suspending(&field.value, suspending))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_calls_suspending(base, suspending)
                || fields
                    .iter()
                    .any(|field| expr_calls_suspending(&field.value, suspending))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_suspending(base, suspending)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .any(|arg| expr_calls_suspending(arg, suspending))
                || expr_calls_suspending(record, suspending)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            expr_calls_suspending(receiver, suspending)
                || args
                    .iter()
                    .any(|arg| expr_calls_suspending(arg, suspending))
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            expr_calls_suspending(expr, suspending)
                || generators
                    .iter()
                    .any(|generator| expr_calls_suspending(&generator.source, suspending))
                || guards
                    .iter()
                    .any(|guard| expr_calls_suspending(guard, suspending))
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
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            expr_calls_suspending(body, suspending)
                || of_clauses.iter().chain(catch_clauses).any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_suspending(guard, suspending))
                        || expr_calls_suspending(&clause.body, suspending)
                })
                || after_clause.as_ref().is_some_and(|after| {
                    expr_calls_suspending(&after.trigger, suspending)
                        || expr_calls_suspending(&after.body, suspending)
                })
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .any(|parameter| expr_calls_suspending(parameter, suspending)),
        _ => false,
    }
}
