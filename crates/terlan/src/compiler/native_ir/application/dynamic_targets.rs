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
        CoreExpr::Lam { params, body } => Some((params.len(), body)),
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

pub(in crate::compiler::native_ir) fn walk_calls(
    expr: &CoreExpr,
    visit: &mut impl FnMut(&str, &[CoreExpr]),
) {
    if let CoreExpr::Call { function, args } = expr {
        visit(function, args);
    }
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().for_each(|argument| walk_calls(argument, visit));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            walk_calls(receiver, visit);
            args.iter().for_each(|argument| walk_calls(argument, visit));
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().for_each(|item| walk_calls(item, visit));
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
            walk_calls(head, visit);
            walk_calls(tail, visit);
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .for_each(|field| walk_calls(&field.value, visit)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .for_each(|field| walk_calls(&field.value, visit))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            walk_calls(base, visit);
            fields
                .iter()
                .for_each(|field| walk_calls(&field.value, visit));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => walk_calls(base, visit),
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .for_each(|binding| walk_calls(&binding.value, visit));
            walk_calls(body, visit);
        }
        CoreExpr::If { clauses } => clauses.iter().for_each(|clause| {
            walk_calls(&clause.condition, visit);
            walk_calls(&clause.body, visit);
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            walk_calls(scrutinee, visit);
            clauses.iter().for_each(|clause| {
                if let Some(guard) = &clause.guard {
                    walk_calls(guard, visit);
                }
                walk_calls(&clause.body, visit);
            });
        }
        CoreExpr::Lam { body, .. } => walk_calls(body, visit),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().for_each(|argument| walk_calls(argument, visit));
            walk_calls(record, visit);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            walk_calls(expr, visit);
            generators
                .iter()
                .for_each(|generator| walk_calls(&generator.source, visit));
            guards.iter().for_each(|guard| walk_calls(guard, visit));
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            walk_calls(body, visit);
            of_clauses.iter().chain(catch_clauses).for_each(|clause| {
                if let Some(guard) = &clause.guard {
                    walk_calls(guard, visit);
                }
                walk_calls(&clause.body, visit);
            });
            if let Some(after) = after_clause {
                walk_calls(&after.trigger, visit);
                walk_calls(&after.body, visit);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .for_each(|parameter| walk_calls(parameter, visit)),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}
