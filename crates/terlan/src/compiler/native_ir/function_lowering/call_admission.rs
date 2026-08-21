// Native-call admission kept separate from the primary lowering orchestration.

use super::*;

pub(in super::super) fn expr_calls_are_supported(
    expr: &CoreExpr,
    identities: &[(&str, usize)],
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
    tail_position: bool,
) -> bool {
    if suspending_call_count(expr, suspending) > MAX_NATIVE_CALL_COMPOSITION_DEPTH {
        return false;
    }
    let is_composable =
        |function: &str, arity: usize| composable.contains(&(function.to_string(), arity));
    if let Some(region) = composed_call_region(expr, suspending, &is_composable, &HashSet::new()) {
        // `composed_call_region` proves the next evaluation context, and the
        // resume is lowered recursively by `lower_owned_expr_with_yields`.
        // Revalidating the whole resume through the narrower scalar-control
        // predicate rejects valid grouped-let/case continuations. Local-call
        // closure plus the region's explicit gate contracts are the matching
        // admission proof; the recursive lowerer remains authoritative for
        // every later suspension context.
        return region.gates.iter().all(|gate| {
            expr_is_scalar(&gate.condition)
                    && gate.prefix.iter().all(|binding| {
                        !expr_calls_suspending(&binding.value, suspending)
                            && expr_calls_are_local(&binding.value, identities)
                    })
                    && expr_is_native_control(&gate.bypass_resume)
                    // A gate bypass is the same surrounding continuation as
                    // `region.resume`, with only the short-circuit result
                    // substituted. Recursing into every bypass revalidates
                    // the shared suffix once per boolean term and makes
                    // admission exponential for long assertion pipelines.
                    && expr_calls_are_local(&gate.bypass_resume, identities)
        }) && expr_calls_are_local(expr, identities);
    }
    if let Some(region) = condition_yield_region(expr) {
        return region.prefix.iter().all(|binding| {
            !expr_calls_suspending(&binding.value, suspending)
                && expr_calls_are_local(&binding.value, identities)
        }) && expr_calls_are_supported(
            &region.resume,
            identities,
            suspending,
            composable,
            tail_position,
        );
    }
    match expr {
        CoreExpr::Call { function, args } => {
            let identity = (function.clone(), args.len());
            let is_local = identities
                .iter()
                .any(|(name, arity)| *name == function && *arity == args.len());
            is_local
                && (!suspending.contains(&identity) || tail_position)
                && args.iter().all(|arg| {
                    !expr_calls_suspending(arg, suspending) && expr_calls_are_local(arg, identities)
                })
        }
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().all(|arg| {
                !expr_calls_suspending(arg, suspending) && expr_calls_are_local(arg, identities)
            })
        }
        CoreExpr::RecordConstruct { fields, .. } => fields.iter().all(|field| {
            !expr_calls_suspending(&field.value, suspending)
                && expr_calls_are_local(&field.value, identities)
        }),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            !expr_calls_suspending(base, suspending)
                && expr_calls_are_local(base, identities)
                && fields.iter().all(|field| {
                    !expr_calls_suspending(&field.value, suspending)
                        && expr_calls_are_local(&field.value, identities)
                })
        }
        CoreExpr::UnaryOp { operand, .. } => {
            expr_calls_are_supported(operand, identities, suspending, composable, false)
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_are_supported(base, identities, suspending, composable, false)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_calls_are_supported(left, identities, suspending, composable, false)
                && !expr_calls_suspending(right, suspending)
                && expr_calls_are_local(right, identities)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                (!expr_calls_suspending(&binding.value, suspending)
                    && expr_calls_are_local(&binding.value, identities))
                    || (expr_is_native_control(&binding.value)
                        && expr_calls_are_supported(
                            &binding.value,
                            identities,
                            suspending,
                            composable,
                            false,
                        ))
            }) && expr_calls_are_supported(body, identities, suspending, composable, tail_position)
        }
        CoreExpr::If { clauses } => clauses.iter().all(|clause| {
            expr_calls_are_supported(&clause.condition, identities, suspending, composable, false)
                && expr_calls_are_supported(
                    &clause.body,
                    identities,
                    suspending,
                    composable,
                    tail_position,
                )
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            expr_calls_are_supported(scrutinee, identities, suspending, composable, false)
                && clauses.iter().all(|clause| {
                    clause.guard.as_ref().is_none_or(|guard| {
                        expr_calls_are_supported(guard, identities, suspending, composable, false)
                    }) && expr_calls_are_supported(
                        &clause.body,
                        identities,
                        suspending,
                        composable,
                        tail_position,
                    )
                })
        }
        CoreExpr::Lam { body, .. } => {
            expr_calls_are_supported(body, identities, suspending, composable, true)
        }
        _ => true,
    }
}
