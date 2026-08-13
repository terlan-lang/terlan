//! Interprocedural scalar replacement for private projection-only helpers.

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreLetBinding, CoreModule, CorePattern};

use super::native_type;

/// Rewrites eligible private helpers and removes their now-unreachable ABI.
#[cfg(test)]
pub(super) fn specialize_projection_callees(cores: &mut [CoreModule]) -> Result<(), String> {
    let mut budget = super::specialization_budget::SpecializationBudget::default();
    specialize_projection_callees_with_budget(cores, &mut budget)
}

/// Rewrites eligible projection helpers under one application-wide budget.
pub(super) fn specialize_projection_callees_with_budget(
    cores: &mut [CoreModule],
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    for core in cores {
        loop {
            let candidates = core
                .functions
                .iter()
                .filter_map(|function| projection_callee(function, &core.module))
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                break;
            }
            let mut applied = false;
            for candidate in candidates {
                let mut rewritten = core.clone();
                let mut ordinal = 0_u64;
                let mut call_count = 0_usize;
                let mut valid = true;
                for function in &mut rewritten.functions {
                    for clause in &mut function.clauses {
                        if let Some(guard) = &mut clause.guard {
                            if let Some(expr) = &mut guard.core_expr {
                                valid &=
                                    rewrite_calls(expr, &candidate, &mut ordinal, &mut call_count);
                            }
                        }
                        if let Some(expr) = &mut clause.body.core_expr {
                            valid &= rewrite_calls(expr, &candidate, &mut ordinal, &mut call_count);
                        }
                    }
                }
                valid &= !module_uses_callee(&rewritten, &candidate);
                if !valid || call_count == 0 {
                    continue;
                }
                budget.reserve(
                    super::specialization_budget::SpecializationKind::Projection,
                    &core.module,
                    call_count,
                )?;
                rewritten.functions.retain(|function| {
                    function.name != candidate.function || function.arity != candidate.arity
                });
                *core = rewritten;
                applied = true;
                break;
            }
            if !applied {
                break;
            }
        }
    }
    Ok(())
}

/// One private helper proven removable after call-site rewriting.
#[derive(Clone)]
struct ProjectionCallee {
    /// Source function name used by local CoreIR calls.
    function: String,
    /// Closed-application identity used after call normalization.
    qualified_function: String,
    /// Original function arity, currently fixed to one managed parameter.
    arity: usize,
    /// Managed parameter name projected by the helper body.
    parameter: String,
    /// Typed helper body cloned into each supported call site.
    body: CoreExpr,
}

/// Accepts both source-local and closed-application call identities.
fn addresses_candidate(function: &str, candidate: &ProjectionCallee) -> bool {
    function == candidate.function || function == candidate.qualified_function
}

/// Recognizes one single-clause private helper that only projects its argument.
fn projection_callee(function: &CoreFunction, module: &str) -> Option<ProjectionCallee> {
    if function.public
        || function.native_operation.is_some()
        || function.arity != 1
        || function.params.len() != 1
        || !native_type(function.params[0].core_ty.as_ref(), &function.params[0].ty)?
            .is_managed_reference()
    {
        return None;
    }
    let [clause] = function.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some()
        || !matches!(
            clause.core_patterns.as_slice(),
            [Some(CorePattern::Var(name))] if name == &function.params[0].name
        )
    {
        return None;
    }
    let body = clause.body.core_expr.as_ref()?;
    let mut projections = 0_usize;
    substitute_projection_parameter(
        body,
        &function.params[0].name,
        &CoreExpr::Var("$native_callee_probe".to_owned()),
        &mut projections,
    )?;
    (projections > 0).then(|| ProjectionCallee {
        function: function.name.clone(),
        qualified_function: format!("{module}.{}", function.name),
        arity: function.arity,
        parameter: function.params[0].name.clone(),
        body: body.clone(),
    })
}

/// Rewrites every direct call while rejecting unresolved references or recursion.
fn rewrite_calls(
    expr: &mut CoreExpr,
    candidate: &ProjectionCallee,
    ordinal: &mut u64,
    call_count: &mut usize,
) -> bool {
    match expr {
        CoreExpr::Call { function, args }
            if addresses_candidate(function, candidate) && args.len() == candidate.arity =>
        {
            let argument = args[0].clone();
            let replacement = match &argument {
                CoreExpr::Var(name) => CoreExpr::Var(name.clone()),
                argument if is_fixed_aggregate_argument(argument) => {
                    let name = format!("$native_callee_sroa_{}", *ordinal);
                    *ordinal = ordinal.saturating_add(1);
                    CoreExpr::Var(name)
                }
                _ => return false,
            };
            let mut projections = 0_usize;
            let Some(body) = substitute_projection_parameter(
                &candidate.body,
                &candidate.parameter,
                &replacement,
                &mut projections,
            ) else {
                return false;
            };
            *call_count = call_count.saturating_add(1);
            *expr = if let CoreExpr::Var(name) = replacement {
                if matches!(argument, CoreExpr::Var(_)) {
                    body
                } else {
                    CoreExpr::Let {
                        bindings: vec![CoreLetBinding {
                            pattern: CorePattern::Var(name),
                            value: argument,
                        }],
                        body: Box::new(body),
                    }
                }
            } else {
                unreachable!("callee replacement always uses one local variable")
            };
            true
        }
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. } => args
            .iter_mut()
            .all(|arg| rewrite_calls(arg, candidate, ordinal, call_count)),
        CoreExpr::Intrinsic(call) => call
            .args
            .iter_mut()
            .all(|arg| rewrite_calls(arg, candidate, ordinal, call_count)),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter_mut()
            .all(|item| rewrite_calls(item, candidate, ordinal, call_count)),
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .all(|field| rewrite_calls(&mut field.value, candidate, ordinal, call_count)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .all(|field| rewrite_calls(&mut field.value, candidate, ordinal, call_count))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite_calls(base, candidate, ordinal, call_count)
                && fields
                    .iter_mut()
                    .all(|field| rewrite_calls(&mut field.value, candidate, ordinal, call_count))
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            rewrite_calls(head, candidate, ordinal, call_count)
                && rewrite_calls(tail, candidate, ordinal, call_count)
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => rewrite_calls(base, candidate, ordinal, call_count),
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite_calls(expr, candidate, ordinal, call_count)
                && generators.iter_mut().all(|generator| {
                    rewrite_calls(&mut generator.source, candidate, ordinal, call_count)
                })
                && guards
                    .iter_mut()
                    .all(|guard| rewrite_calls(guard, candidate, ordinal, call_count))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .all(|binding| rewrite_calls(&mut binding.value, candidate, ordinal, call_count))
                && rewrite_calls(body, candidate, ordinal, call_count)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut()
                .all(|arg| rewrite_calls(arg, candidate, ordinal, call_count))
                && rewrite_calls(record, candidate, ordinal, call_count)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite_calls(receiver, candidate, ordinal, call_count)
                && args
                    .iter_mut()
                    .all(|arg| rewrite_calls(arg, candidate, ordinal, call_count))
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite_calls(callee, candidate, ordinal, call_count)
                && args
                    .iter_mut()
                    .all(|arg| rewrite_calls(arg, candidate, ordinal, call_count))
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter_mut()
            .all(|parameter| rewrite_calls(parameter, candidate, ordinal, call_count)),
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite_calls(scrutinee, candidate, ordinal, call_count)
                && clauses.iter_mut().all(|clause| {
                    clause
                        .guard
                        .as_mut()
                        .is_none_or(|guard| rewrite_calls(guard, candidate, ordinal, call_count))
                        && rewrite_calls(&mut clause.body, candidate, ordinal, call_count)
                })
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite_calls(body, candidate, ordinal, call_count)
                && of_clauses
                    .iter_mut()
                    .chain(catch_clauses.iter_mut())
                    .all(|clause| {
                        clause.guard.as_mut().is_none_or(|guard| {
                            rewrite_calls(guard, candidate, ordinal, call_count)
                        }) && rewrite_calls(&mut clause.body, candidate, ordinal, call_count)
                    })
                && after_clause.as_mut().is_none_or(|after| {
                    rewrite_calls(&mut after.trigger, candidate, ordinal, call_count)
                        && rewrite_calls(&mut after.body, candidate, ordinal, call_count)
                })
        }
        CoreExpr::If { clauses } => clauses.iter_mut().all(|clause| {
            rewrite_calls(&mut clause.condition, candidate, ordinal, call_count)
                && rewrite_calls(&mut clause.body, candidate, ordinal, call_count)
        }),
        CoreExpr::RemoteFunRef {
            function, arity, ..
        } if addresses_candidate(function, candidate) && *arity == candidate.arity => false,
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => true,
    }
}

/// Accepts fixed aggregate values through type-only CoreIR casts.
fn is_fixed_aggregate_argument(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::ConstructorCall { .. }
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::Tuple(_)
        | CoreExpr::FixedArray(_) => true,
        CoreExpr::Cast { expr, .. } => is_fixed_aggregate_argument(expr),
        _ => false,
    }
}

/// Reports whether a rewritten module still addresses the removable helper.
fn module_uses_callee(module: &CoreModule, candidate: &ProjectionCallee) -> bool {
    module.functions.iter().any(|function| {
        function.clauses.iter().any(|clause| {
            clause
                .guard
                .as_ref()
                .and_then(|guard| guard.core_expr.as_ref())
                .is_some_and(|expr| expr_uses_callee(expr, candidate))
                || clause
                    .body
                    .core_expr
                    .as_ref()
                    .is_some_and(|expr| expr_uses_callee(expr, candidate))
        })
    })
}

/// Reports whether an expression contains a direct call or reference to a helper.
fn expr_uses_callee(expr: &CoreExpr, candidate: &ProjectionCallee) -> bool {
    match expr {
        CoreExpr::Call { function, args } => {
            (addresses_candidate(function, candidate) && args.len() == candidate.arity)
                || args.iter().any(|arg| expr_uses_callee(arg, candidate))
        }
        CoreExpr::RemoteFunRef {
            function, arity, ..
        } => addresses_candidate(function, candidate) && *arity == candidate.arity,
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            args.iter().any(|arg| expr_uses_callee(arg, candidate))
        }
        CoreExpr::Intrinsic(call) => call.args.iter().any(|arg| expr_uses_callee(arg, candidate)),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().any(|item| expr_uses_callee(item, candidate))
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .any(|field| expr_uses_callee(&field.value, candidate)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .any(|field| expr_uses_callee(&field.value, candidate))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_uses_callee(base, candidate)
                || fields
                    .iter()
                    .any(|field| expr_uses_callee(&field.value, candidate))
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => expr_uses_callee(head, candidate) || expr_uses_callee(tail, candidate),
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => expr_uses_callee(base, candidate),
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            expr_uses_callee(expr, candidate)
                || generators
                    .iter()
                    .any(|generator| expr_uses_callee(&generator.source, candidate))
                || guards
                    .iter()
                    .any(|guard| expr_uses_callee(guard, candidate))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expr_uses_callee(&binding.value, candidate))
                || expr_uses_callee(body, candidate)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().any(|arg| expr_uses_callee(arg, candidate))
                || expr_uses_callee(record, candidate)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            expr_uses_callee(receiver, candidate)
                || args.iter().any(|arg| expr_uses_callee(arg, candidate))
        }
        CoreExpr::FunctionCall { callee, args } => {
            expr_uses_callee(callee, candidate)
                || args.iter().any(|arg| expr_uses_callee(arg, candidate))
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .any(|parameter| expr_uses_callee(parameter, candidate)),
        CoreExpr::Case { scrutinee, clauses } => {
            expr_uses_callee(scrutinee, candidate)
                || clauses.iter().any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_uses_callee(guard, candidate))
                        || expr_uses_callee(&clause.body, candidate)
                })
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            expr_uses_callee(body, candidate)
                || of_clauses.iter().chain(catch_clauses.iter()).any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_uses_callee(guard, candidate))
                        || expr_uses_callee(&clause.body, candidate)
                })
                || after_clause.as_ref().is_some_and(|after| {
                    expr_uses_callee(&after.trigger, candidate)
                        || expr_uses_callee(&after.body, candidate)
                })
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expr_uses_callee(&clause.condition, candidate)
                || expr_uses_callee(&clause.body, candidate)
        }),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_) => false,
    }
}

/// Substitutes a parameter only where the helper performs named projections.
fn substitute_projection_parameter(
    expr: &CoreExpr,
    target: &str,
    replacement: &CoreExpr,
    projections: &mut usize,
) -> Option<CoreExpr> {
    match expr {
        CoreExpr::Var(name) if name == target => None,
        CoreExpr::FieldAccess { base, field } if matches!(base.as_ref(), CoreExpr::Var(name) if name == target) =>
        {
            *projections = projections.saturating_add(1);
            Some(CoreExpr::FieldAccess {
                base: Box::new(replacement.clone()),
                field: field.clone(),
            })
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => Some(CoreExpr::ConstructorCall {
            constructor: constructor.clone(),
            constructor_identity: constructor_identity.clone(),
            args: substitute_projection_args(args, target, replacement, projections)?,
        }),
        CoreExpr::Call { function, args } => Some(CoreExpr::Call {
            function: function.clone(),
            args: substitute_projection_args(args, target, replacement, projections)?,
        }),
        CoreExpr::Intrinsic(call) => {
            let mut call = call.clone();
            call.args = substitute_projection_args(&call.args, target, replacement, projections)?;
            Some(CoreExpr::Intrinsic(call))
        }
        CoreExpr::UnaryOp { operator, operand } => Some(CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(substitute_projection_parameter(
                operand,
                target,
                replacement,
                projections,
            )?),
        }),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => Some(CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(substitute_projection_parameter(
                left,
                target,
                replacement,
                projections,
            )?),
            right: Box::new(substitute_projection_parameter(
                right,
                target,
                replacement,
                projections,
            )?),
        }),
        CoreExpr::Let { bindings, body }
            if bindings
                .iter()
                .all(|binding| !pattern_binds(&binding.pattern, target)) =>
        {
            Some(CoreExpr::Let {
                bindings: bindings
                    .iter()
                    .map(|binding| {
                        Some(CoreLetBinding {
                            pattern: binding.pattern.clone(),
                            value: substitute_projection_parameter(
                                &binding.value,
                                target,
                                replacement,
                                projections,
                            )?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
                body: Box::new(substitute_projection_parameter(
                    body,
                    target,
                    replacement,
                    projections,
                )?),
            })
        }
        CoreExpr::If { clauses } => Some(CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = substitute_projection_parameter(
                        &clause.condition,
                        target,
                        replacement,
                        projections,
                    )?;
                    clause.body = substitute_projection_parameter(
                        &clause.body,
                        target,
                        replacement,
                        projections,
                    )?;
                    Some(clause)
                })
                .collect::<Option<Vec<_>>>()?,
        }),
        CoreExpr::Cast { expr, target_type } => Some(CoreExpr::Cast {
            expr: Box::new(substitute_projection_parameter(
                expr,
                target,
                replacement,
                projections,
            )?),
            target_type: target_type.clone(),
        }),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_) => Some(expr.clone()),
        _ => None,
    }
}

/// Substitutes one projection parameter through an ordered argument vector.
fn substitute_projection_args(
    args: &[CoreExpr],
    target: &str,
    replacement: &CoreExpr,
    projections: &mut usize,
) -> Option<Vec<CoreExpr>> {
    args.iter()
        .map(|arg| substitute_projection_parameter(arg, target, replacement, projections))
        .collect()
}

/// Reports whether one supported binding pattern shadows a parameter.
fn pattern_binds(pattern: &CorePattern, target: &str) -> bool {
    match pattern {
        CorePattern::Var(name) => name == target,
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => patterns
            .iter()
            .any(|pattern| pattern_binds(pattern, target)),
        CorePattern::Alias { alias, pattern } => alias == target || pattern_binds(pattern, target),
        CorePattern::ListCons { head, tail } => {
            pattern_binds(head, target) || pattern_binds(tail, target)
        }
        CorePattern::Map(fields) => fields
            .iter()
            .any(|field| pattern_binds(&field.value, target)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .any(|field| pattern_binds(&field.value, target)),
        CorePattern::Constructor { args, .. } => {
            args.iter().any(|pattern| pattern_binds(pattern, target))
        }
        _ => false,
    }
}
