//! Closed-world callback target flow for call-site-specialized helpers.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreType};

use super::{CallIdentity, Candidate};

#[cfg(test)]
#[path = "dynamic_targets_test.rs"]
mod tests;

type Parameter = (usize, usize);

pub(super) fn restrict_profiles(
    profiles: &super::super::call_composition::DynamicCallProfiles,
    allowed: Option<&HashSet<u64>>,
) -> super::super::call_composition::DynamicCallProfiles {
    let Some(allowed) = allowed.filter(|allowed| !allowed.is_empty()) else {
        return profiles.clone();
    };
    profiles
        .iter()
        .filter_map(|(signature, targets)| {
            let matching = targets
                .iter()
                .filter(|target| allowed.contains(&target.export_id))
                .cloned()
                .collect::<Vec<_>>();
            (!matching.is_empty()).then(|| (signature.clone(), matching))
        })
        .collect()
}

pub(super) fn validate_profiles(
    profiles: &super::super::call_composition::DynamicCallProfiles,
    allowed: Option<&HashSet<u64>>,
    owner: &str,
    gaps: &HashMap<u64, String>,
) -> Result<(), super::super::NativeIrError> {
    let Some(allowed) = allowed.filter(|allowed| !allowed.is_empty()) else {
        return Ok(());
    };
    let available_profiles = profiles
        .values()
        .flatten()
        .map(|target| (target.export_id, target.source.clone()))
        .collect::<Vec<_>>();
    let available = available_profiles
        .iter()
        .map(|(export_id, _)| *export_id)
        .collect::<HashSet<_>>();
    let mut missing = allowed.difference(&available).copied().collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort_unstable();
    let mut available_profiles = available_profiles;
    available_profiles.sort_by_key(|(export_id, _)| *export_id);
    let missing_reasons = missing
        .iter()
        .filter_map(|export_id| {
            gaps.get(export_id)
                .map(|reason| (*export_id, reason.as_str()))
        })
        .collect::<Vec<_>>();
    Err(format!(
        "error[native_ir.dynamic_target_profile]: `{owner}` requires closure targets {missing:?} without suspension profiles; gaps {missing_reasons:?}; available targets {available_profiles:?}"
    )
    .into())
}

pub(super) fn candidate_parameter_targets(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
) -> HashMap<usize, HashSet<u64>> {
    let closure_results = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let body = candidate
                .function
                .clauses
                .first()?
                .body
                .core_expr
                .as_ref()?;
            let (lambda_params, lambda_body) = returned_lambda(body)?;
            let free = super::super::free_variables(lambda_body);
            let capture_count = candidate
                .function
                .params
                .iter()
                .filter(|parameter| free.contains(&parameter.name))
                .count();
            let name = format!(
                "$closure_{}_{}_0",
                candidate.function.name, candidate.function.arity
            );
            Some((
                index,
                super::super::stable_export_id(
                    &candidate.core.module,
                    &name,
                    capture_count.saturating_add(lambda_params),
                ),
            ))
        })
        .collect::<HashMap<_, _>>();
    let mut targets = HashMap::<Parameter, HashSet<u64>>::new();
    let mut forwards = Vec::<(Parameter, Parameter)>::new();
    for (caller_index, caller) in candidates.iter().enumerate() {
        if !selected[caller_index] {
            continue;
        }
        let Some(body) = caller
            .function
            .clauses
            .first()
            .and_then(|clause| clause.body.core_expr.as_ref())
        else {
            continue;
        };
        walk_calls(body, &mut |function, args| {
            let resolver = &resolvers[&caller.core.module];
            let Some(callee_index) = resolver.get(&(function.to_string(), args.len())).copied()
            else {
                return;
            };
            let callee = &candidates[callee_index];
            for (parameter_index, (parameter, argument)) in
                callee.function.params.iter().zip(args).enumerate()
            {
                let Some(CoreType::Arrow {
                    params: callback_params,
                    ..
                }) = parameter.core_ty.as_ref()
                else {
                    continue;
                };
                let destination = (callee_index, parameter_index);
                collect_argument_targets(
                    argument,
                    TargetCollection {
                        callback_arity: callback_params.len(),
                        caller_index,
                        caller,
                        resolver,
                        candidates,
                        closure_results: &closure_results,
                        destination,
                    },
                    targets.entry(destination).or_default(),
                    &mut forwards,
                );
            }
        });
    }
    loop {
        let mut changed = false;
        for (source, destination) in &forwards {
            let incoming = targets.get(source).cloned().unwrap_or_default();
            let destination = targets.entry(*destination).or_default();
            let before = destination.len();
            destination.extend(incoming);
            changed |= destination.len() != before;
        }
        if !changed {
            break;
        }
    }
    let mut by_candidate = HashMap::<usize, HashSet<u64>>::new();
    for ((candidate, _), values) in targets {
        by_candidate.entry(candidate).or_default().extend(values);
    }
    by_candidate
}

fn returned_lambda(expr: &CoreExpr) -> Option<(usize, &CoreExpr)> {
    match expr {
        CoreExpr::Lam { params, body, .. } => Some((params.len(), body)),
        CoreExpr::Let { body, .. } | CoreExpr::Cast { expr: body, .. } => returned_lambda(body),
        CoreExpr::If { clauses } => clauses
            .iter()
            .find_map(|clause| returned_lambda(&clause.body)),
        _ => None,
    }
}

struct TargetCollection<'a> {
    callback_arity: usize,
    caller_index: usize,
    caller: &'a Candidate<'a>,
    resolver: &'a HashMap<CallIdentity, usize>,
    candidates: &'a [Candidate<'a>],
    closure_results: &'a HashMap<usize, u64>,
    destination: Parameter,
}

fn collect_argument_targets(
    argument: &CoreExpr,
    context: TargetCollection<'_>,
    concrete: &mut HashSet<u64>,
    forwards: &mut Vec<(Parameter, Parameter)>,
) {
    let TargetCollection {
        callback_arity,
        caller_index,
        caller,
        resolver,
        candidates,
        closure_results,
        destination,
    } = context;
    match argument {
        CoreExpr::Call { function, args } => {
            if let Some(owner) = resolver.get(&(function.clone(), args.len())) {
                if let Some(target) = closure_results.get(owner) {
                    concrete.insert(*target);
                }
            }
        }
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } if *arity == callback_arity => {
            concrete.insert(super::super::stable_export_id(module, function, *arity));
        }
        CoreExpr::Var(name) => {
            if let Some(parameter_index) = caller
                .function
                .params
                .iter()
                .position(|parameter| parameter.name == *name)
            {
                forwards.push(((caller_index, parameter_index), destination));
            } else if let Some(target) = resolver
                .get(&(name.clone(), callback_arity))
                .and_then(|candidate| candidates.get(*candidate))
            {
                concrete.insert(super::super::stable_export_id(
                    &target.core.module,
                    &target.function.name,
                    target.function.arity,
                ));
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_argument_targets(
                    &clause.body,
                    TargetCollection {
                        callback_arity,
                        caller_index,
                        caller,
                        resolver,
                        candidates,
                        closure_results,
                        destination,
                    },
                    concrete,
                    forwards,
                );
            }
        }
        CoreExpr::Cast { expr, .. } => collect_argument_targets(
            expr,
            TargetCollection {
                callback_arity,
                caller_index,
                caller,
                resolver,
                candidates,
                closure_results,
                destination,
            },
            concrete,
            forwards,
        ),
        _ => {}
    }
}

/// Visits ordinary calls using the same exhaustive traversal as admission.
pub(in crate::compiler::native_ir) fn walk_calls(
    expr: &CoreExpr,
    visit: &mut impl FnMut(&str, &[CoreExpr]),
) {
    walk_expressions(expr, &mut |expression| {
        if let CoreExpr::Call { function, args } = expression {
            visit(function, args);
        }
    });
}

/// Visits every expression, including guards, casts, and callable references.
pub(in crate::compiler::native_ir) fn walk_expressions(
    expr: &CoreExpr,
    visit: &mut impl FnMut(&CoreExpr),
) {
    visit(expr);
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter()
                .for_each(|argument| walk_expressions(argument, visit));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            walk_expressions(receiver, visit);
            args.iter()
                .for_each(|argument| walk_expressions(argument, visit));
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().for_each(|item| walk_expressions(item, visit));
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
            walk_expressions(head, visit);
            walk_expressions(tail, visit);
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .for_each(|field| walk_expressions(&field.value, visit)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .for_each(|field| walk_expressions(&field.value, visit))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            walk_expressions(base, visit);
            fields
                .iter()
                .for_each(|field| walk_expressions(&field.value, visit));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => walk_expressions(base, visit),
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .for_each(|binding| walk_expressions(&binding.value, visit));
            walk_expressions(body, visit);
        }
        CoreExpr::If { clauses } => clauses.iter().for_each(|clause| {
            walk_expressions(&clause.condition, visit);
            walk_expressions(&clause.body, visit);
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            walk_expressions(scrutinee, visit);
            clauses.iter().for_each(|clause| {
                if let Some(guard) = &clause.guard {
                    walk_expressions(guard, visit);
                }
                walk_expressions(&clause.body, visit);
            });
        }
        CoreExpr::Lam { body, .. } => walk_expressions(body, visit),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .for_each(|argument| walk_expressions(argument, visit));
            walk_expressions(record, visit);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            walk_expressions(expr, visit);
            generators
                .iter()
                .for_each(|generator| walk_expressions(&generator.source, visit));
            guards
                .iter()
                .for_each(|guard| walk_expressions(guard, visit));
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            walk_expressions(body, visit);
            of_clauses.iter().chain(catch_clauses).for_each(|clause| {
                if let Some(guard) = &clause.guard {
                    walk_expressions(guard, visit);
                }
                walk_expressions(&clause.body, visit);
            });
            if let Some(after) = after_clause {
                walk_expressions(&after.trigger, visit);
                walk_expressions(&after.body, visit);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .for_each(|parameter| walk_expressions(parameter, visit)),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}
