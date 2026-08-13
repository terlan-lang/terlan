//! Callable normalization before application lowering.

use super::*;

pub(super) fn normalize_static_callables(
    core: &mut CoreModule,
    budget: &mut super::super::specialization_budget::SpecializationBudget,
) -> Result<(), super::super::NativeIrError> {
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = super::super::static_callable::normalize_static_callables_with_budget(
                    body,
                    &core.module,
                    budget,
                )?;
            }
        }
    }
    Ok(())
}

/// Reifies calls through closure-typed lexical aliases after type checking.
pub(super) fn normalize_dynamic_callable_aliases(core: &mut CoreModule) {
    for function in &mut core.functions {
        let closures = function
            .params
            .iter()
            .filter(|parameter| matches!(parameter.core_ty, Some(CoreType::Arrow { .. })))
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                normalize_dynamic_alias_expr(body, &closures);
            }
        }
    }
}

fn normalize_dynamic_alias_expr(expr: &mut CoreExpr, closures: &HashSet<String>) {
    match expr {
        CoreExpr::Call { function, args } if closures.contains(function) => {
            for argument in args.iter_mut() {
                normalize_dynamic_alias_expr(argument, closures);
            }
            *expr = CoreExpr::FunctionCall {
                callee: Box::new(CoreExpr::Var(function.clone())),
                args: std::mem::take(args),
            };
        }
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for argument in args {
                normalize_dynamic_alias_expr(argument, closures);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            normalize_dynamic_alias_expr(callee, closures);
            for argument in args {
                normalize_dynamic_alias_expr(argument, closures);
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut nested = closures.clone();
            for binding in bindings {
                normalize_dynamic_alias_expr(&mut binding.value, &nested);
                if let CorePattern::Var(name) = &binding.pattern {
                    if closure_alias_value(&binding.value, &nested) {
                        nested.insert(name.clone());
                    } else {
                        nested.remove(name);
                    }
                }
            }
            normalize_dynamic_alias_expr(body, &nested);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                normalize_dynamic_alias_expr(&mut clause.condition, closures);
                normalize_dynamic_alias_expr(&mut clause.body, closures);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            normalize_dynamic_alias_expr(scrutinee, closures);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    normalize_dynamic_alias_expr(guard, closures);
                }
                normalize_dynamic_alias_expr(&mut clause.body, closures);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            normalize_dynamic_alias_expr(body, closures);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    normalize_dynamic_alias_expr(guard, closures);
                }
                normalize_dynamic_alias_expr(&mut clause.body, closures);
            }
            if let Some(after) = after_clause {
                normalize_dynamic_alias_expr(&mut after.trigger, closures);
                normalize_dynamic_alias_expr(&mut after.body, closures);
            }
        }
        CoreExpr::UnaryOp { operand, .. } => normalize_dynamic_alias_expr(operand, closures),
        CoreExpr::BinaryOp { left, right, .. } => {
            normalize_dynamic_alias_expr(left, closures);
            normalize_dynamic_alias_expr(right, closures);
        }
        _ => {}
    }
}

fn closure_alias_value(value: &CoreExpr, closures: &HashSet<String>) -> bool {
    match value {
        CoreExpr::Var(name) => closures.contains(name),
        CoreExpr::RemoteFunRef { .. } | CoreExpr::Lam { .. } => true,
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| closure_alias_value(&clause.body, closures))
        }
        _ => false,
    }
}
