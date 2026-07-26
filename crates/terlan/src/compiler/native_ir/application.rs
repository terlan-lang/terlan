//! Application-wide NativeIR admission and symbol resolution.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreImportKind, CoreModule, CorePattern, CoreType,
};

use super::{
    aggregate_types::{managed_aggregate_layouts, managed_expression_layouts},
    atom_inventory::application_atom_identities,
    collections::{managed_collection_layouts, managed_expression_collection_layouts},
    constructors::native_constructor_layouts,
    contains_process_yield, expr_calls_are_supported, is_composable_suspending_body,
    is_scalar_candidate, native_return_type_with_constructors, native_type_with_constructors,
    scalar_replacement::scalar_replace_fixed_aggregates,
    ComposedCallProfile, NativeCallableShape, NativeContinuation, NativeModule, NativeType,
};

type CallIdentity = (String, usize);

#[path = "application/admission_diagnostics.rs"]
mod admission_diagnostics;
mod native_packages;
mod structural_patterns;
mod transparent_aliases;
use admission_diagnostics::candidate_admission_summary;
use native_packages::{
    lower_compiler_native_declarations, native_handle_layouts, native_package_aliases,
};

#[derive(Clone, Copy)]
struct Candidate<'a> {
    core: &'a CoreModule,
    function: &'a CoreFunction,
}

impl NativeModule {
    /// Lowers one checked CoreIR closure against one application-wide symbol table.
    pub(crate) fn lower_application(cores: &[&CoreModule]) -> Result<Vec<Self>, String> {
        let mut normalized_cores = cores.iter().map(|core| (*core).clone()).collect::<Vec<_>>();
        normalized_cores.sort_by(|left, right| left.module.cmp(&right.module));
        if let Some(duplicate) = normalized_cores
            .windows(2)
            .find(|pair| pair[0].module == pair[1].module)
        {
            return Err(super::application_admission::duplicate_module_diagnostic(
                &duplicate[0].module,
            ));
        }
        normalized_cores
            .iter_mut()
            .for_each(super::nominal_identity::qualify_local_nominal_types);
        super::atom_alias_values::lower_atom_alias_values(&mut normalized_cores);
        let native_aliases = native_package_aliases(&normalized_cores);
        for core in &mut normalized_cores {
            lower_compiler_native_declarations(core, &native_aliases)?;
        }
        normalized_cores
            .iter_mut()
            .for_each(|core| normalize_remote_calls(core, true));
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        let mut specialization_budget =
            super::specialization_budget::SpecializationBudget::default();
        for core in &mut normalized_cores {
            super::constructor_chain::lower_constructor_chains(core);
            super::list_comprehension::lower_list_comprehensions(core)?;
            super::template_values::lower_template_values(core)?;
            super::http_values::lower_http_values(core)?;
            normalize_remote_calls(core, false);
            structural_patterns::scalar_replace(core);
            super::case_lowering::lower_scalar_cases(core)?;
            normalize_remote_calls(core, false);
        }
        super::generic_specialization::specialize_application_generics_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        for core in &mut normalized_cores {
            super::higher_order_specialization::specialize_higher_order_helpers_with_budget(
                core,
                &mut specialization_budget,
            )?;
        }
        super::nested_closure_lifting::lift_nested_closure_arguments(&mut normalized_cores)?;
        for core in &mut normalized_cores {
            normalize_static_callables(core, &mut specialization_budget)?;
            normalize_dynamic_callable_aliases(core);
            normalize_remote_calls(core, false);
            structural_patterns::scalar_replace(core);
            super::case_lowering::lower_scalar_cases(core)?;
            normalize_remote_calls(core, false);
        }
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        super::callee_scalar_replacement::specialize_projection_callees_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        super::typed_empty_lists::annotate_empty_list_arguments(&mut normalized_cores);
        super::short_circuit_normalization::right_associate_short_circuit_chains(
            &mut normalized_cores,
        );
        super::open_std_pruning::prune_unreachable_open_std_functions(&mut normalized_cores);
        let ordered_cores = normalized_cores.iter().collect::<Vec<_>>();
        let constructor_modules = ordered_cores
            .iter()
            .map(|core| (core.module.as_str(), core.constructors.as_slice()))
            .collect::<Vec<_>>();
        let type_modules = ordered_cores
            .iter()
            .map(|core| (core.module.as_str(), core.types.as_slice()))
            .collect::<Vec<_>>();
        let mut constructor_layouts = ordered_cores
            .iter()
            .map(|core| {
                native_constructor_layouts(&constructor_modules, &core.module).and_then(
                    |mut layouts| {
                        super::constructors::install_struct_layouts(
                            &type_modules,
                            &core.module,
                            &mut layouts,
                        )?;
                        Ok((core.module.clone(), layouts))
                    },
                )
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        for core in &ordered_cores {
            super::http_values::install_http_constructors(
                core,
                constructor_layouts
                    .get_mut(&core.module)
                    .expect("constructor layouts built for every ordered module"),
            )?;
        }
        super::application_admission::validate_core_application(
            &normalized_cores,
            &constructor_layouts,
        )?;
        let unsupported = ordered_cores
            .iter()
            .flat_map(|core| {
                core.functions
                    .iter()
                    .filter(|function| {
                        !is_scalar_candidate(function, &constructor_layouts[&core.module])
                    })
                    .map(|function| (core.module.as_str(), function))
            })
            .next();
        if let Some((module, function)) = unsupported {
            let admission = candidate_admission_summary(function, &constructor_layouts[module]);
            return Err(format!(
                "error[native_ir.unsupported_application_function]: `{module}.{}/{}` cannot be lowered into the native application image ({admission}); runtime CoreIR interpretation has been removed",
                function.name, function.arity,
            ));
        }

        let candidates = ordered_cores
            .iter()
            .flat_map(|core| {
                let mut functions = core
                    .functions
                    .iter()
                    .filter(|function| {
                        is_scalar_candidate(function, &constructor_layouts[&core.module])
                    })
                    .collect::<Vec<_>>();
                functions.sort_by(|left, right| {
                    left.name
                        .cmp(&right.name)
                        .then_with(|| left.arity.cmp(&right.arity))
                });
                functions
                    .into_iter()
                    .map(|function| Candidate { core, function })
            })
            .collect::<Vec<_>>();
        let selected = vec![true; candidates.len()];

        loop {
            let resolvers = application_resolvers(&ordered_cores, &candidates, &selected);
            let suspending = application_suspending(&candidates, &selected, &resolvers);
            let composable_candidates =
                application_composable_candidates(&candidates, &selected, &resolvers, &suspending);
            let before = selected.iter().filter(|selected| **selected).count();
            for (index, candidate) in candidates.iter().enumerate() {
                if !selected[index] {
                    continue;
                }
                let resolver = &resolvers[&candidate.core.module];
                let identities = resolver
                    .keys()
                    .map(|(name, arity)| (name.as_str(), *arity))
                    .collect::<Vec<_>>();
                let suspending_names = resolved_names(resolver, &suspending);
                let composable = resolver
                    .iter()
                    .filter(|(_, candidate_index)| composable_candidates.contains(candidate_index))
                    .map(|(identity, _)| identity.clone())
                    .collect::<HashSet<_>>();
                let supported = candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                    .is_some_and(|body| {
                        let body = scalar_replace_fixed_aggregates(
                            body,
                            &constructor_layouts[&candidate.core.module],
                        );
                        expr_calls_are_supported(
                            &body,
                            &identities,
                            &suspending_names,
                            &composable,
                            true,
                        )
                    });
                if !supported {
                    return Err(format!(
                        "error[native_ir.unsupported_application_function]: `{}.{}/{}` cannot be closed over the native application image; runtime CoreIR interpretation has been removed",
                        candidate.core.module, candidate.function.name, candidate.function.arity
                    ));
                }
            }
            if selected.iter().filter(|selected| **selected).count() == before {
                break;
            }
        }

        let modules = lower_selected_application(
            &ordered_cores,
            &candidates,
            &selected,
            &constructor_layouts,
        )?;
        super::application_admission::validate_continuation_graph(&modules)?;
        Ok(modules)
    }
}

/// Eliminates statically known non-escaping function values from one module.
fn normalize_static_callables(
    core: &mut CoreModule,
    budget: &mut super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                *body = super::static_callable::normalize_static_callables_with_budget(
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
fn normalize_dynamic_callable_aliases(core: &mut CoreModule) {
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

fn lower_selected_application(
    cores: &[&CoreModule],
    candidates: &[Candidate<'_>],
    selected: &[bool],
    constructor_layouts: &HashMap<String, super::constructors::NativeConstructorLayouts>,
) -> Result<Vec<NativeModule>, String> {
    let atoms = application_atom_identities(cores);
    let candidate_to_native = selected
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
        .enumerate()
        .map(|(native_index, (candidate_index, _))| (candidate_index, native_index))
        .collect::<HashMap<_, _>>();
    let candidate_resolvers = application_resolvers(cores, candidates, selected);
    let suspending = application_suspending(candidates, selected, &candidate_resolvers);
    let composable_candidates =
        application_composable_candidates(candidates, selected, &candidate_resolvers, &suspending);
    let mut call_profiles = HashMap::new();

    loop {
        let before = call_profiles.len();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            if !selected[candidate_index]
                || call_profiles.contains_key(&candidate_to_native[&candidate_index])
            {
                continue;
            }
            let identities = native_resolver(
                &candidate_resolvers[&candidate.core.module],
                &candidate_to_native,
            );
            let function_types = native_function_types(
                &candidate_resolvers[&candidate.core.module],
                &candidate_to_native,
                candidates,
                &constructor_layouts[&candidate.core.module],
            );
            let function_core_types = native_function_core_types(
                &candidate_resolvers[&candidate.core.module],
                &candidate_to_native,
                candidates,
            );
            let callable_shapes = native_callable_shapes(
                &candidate_resolvers[&candidate.core.module],
                &candidate_to_native,
                candidates,
                &constructor_layouts[&candidate.core.module],
            );
            let suspending_names =
                resolved_names(&candidate_resolvers[&candidate.core.module], &suspending);
            if !composable_candidates.contains(&candidate_index) {
                continue;
            }
            let mut profile_ids = HashSet::new();
            let mut profile_lifted = Vec::new();
            let Ok((mut native, mut continuations)) = super::lower_native_function_with_callables(
                &candidate.core.module,
                candidate.function,
                &identities,
                &function_types,
                &function_core_types,
                &callable_shapes,
                &mut profile_lifted,
                &constructor_layouts[&candidate.core.module],
                &suspending_names,
                &call_profiles,
                &mut profile_ids,
            ) else {
                continue;
            };
            super::continuation_sharing::intern_function_continuations(
                &mut native.body,
                &mut continuations,
            );
            if let Some(profile) = ComposedCallProfile::new(&native.body, &continuations) {
                call_profiles.insert(candidate_to_native[&candidate_index], profile);
            }
        }
        if call_profiles.len() == before {
            break;
        }
    }

    let mut export_ids = HashSet::new();
    let mut modules = Vec::new();
    let mut lifted_functions = Vec::new();
    for core in cores {
        let resolver = &candidate_resolvers[&core.module];
        let identities = native_resolver(resolver, &candidate_to_native);
        let function_types = native_function_types(
            resolver,
            &candidate_to_native,
            candidates,
            &constructor_layouts[&core.module],
        );
        let function_core_types =
            native_function_core_types(resolver, &candidate_to_native, candidates);
        let callable_shapes = native_callable_shapes(
            resolver,
            &candidate_to_native,
            candidates,
            &constructor_layouts[&core.module],
        );
        let suspending_names = resolved_names(resolver, &suspending);
        let mut functions = Vec::new();
        let mut continuations = Vec::<NativeContinuation>::new();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            if !selected[candidate_index] || candidate.core.module != core.module {
                continue;
            }
            let (mut function, mut function_continuations) =
                super::lower_native_function_with_callables(
                    &core.module,
                    candidate.function,
                    &identities,
                    &function_types,
                    &function_core_types,
                    &callable_shapes,
                    &mut lifted_functions,
                    &constructor_layouts[&core.module],
                    &suspending_names,
                    &call_profiles,
                    &mut export_ids,
                )
                .map_err(|error| {
                    format!(
                        "{error}; while lowering `{}.{}/{}`",
                        core.module, candidate.function.name, candidate.function.arity
                    )
                })?;
            super::continuation_sharing::intern_function_continuations(
                &mut function.body,
                &mut function_continuations,
            );
            functions.push(function);
            continuations.append(&mut function_continuations);
        }
        if !functions.is_empty() {
            let inferred_dynamic_returns = candidates
                .iter()
                .enumerate()
                .filter(|(index, candidate)| {
                    selected[*index] && candidate.core.module == core.module
                })
                .filter_map(|(_, candidate)| {
                    super::dynamic_return::inferred_dynamic_return_type(candidate.function)
                })
                .collect::<Vec<_>>();
            let mut managed_layouts = constructor_layouts[&core.module]
                .values()
                .map(|layout| layout.encoded_layout.clone())
                .collect::<Vec<_>>();
            managed_layouts.extend(managed_aggregate_layouts(
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(index, candidate)| {
                        selected[*index] && candidate.core.module == core.module
                    })
                    .flat_map(|(_, candidate)| {
                        candidate
                            .function
                            .params
                            .iter()
                            .filter_map(|parameter| parameter.core_ty.as_ref())
                            .chain(candidate.function.core_return_type.iter())
                    }),
            )?);
            managed_layouts.extend(managed_aggregate_layouts(inferred_dynamic_returns.iter())?);
            managed_layouts.extend(managed_expression_layouts(
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(index, candidate)| {
                        selected[*index] && candidate.core.module == core.module
                    })
                    .flat_map(|(_, candidate)| &candidate.function.clauses)
                    .filter_map(|clause| clause.body.core_expr.as_ref()),
            )?);
            managed_layouts.extend(super::http_values::http_managed_layouts(core)?);
            managed_layouts.extend(native_handle_layouts(core)?);
            managed_layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
            managed_layouts.dedup_by(|left, right| left.as_ref() == right.as_ref());
            let mut managed_collections = managed_collection_layouts(
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(index, candidate)| {
                        selected[*index] && candidate.core.module == core.module
                    })
                    .flat_map(|(_, candidate)| {
                        candidate
                            .function
                            .params
                            .iter()
                            .filter_map(|parameter| parameter.core_ty.as_ref())
                            .chain(candidate.function.core_return_type.iter())
                    }),
            )?;
            managed_collections
                .extend(managed_collection_layouts(inferred_dynamic_returns.iter())?);
            managed_collections.extend(managed_expression_collection_layouts(
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(index, candidate)| {
                        selected[*index] && candidate.core.module == core.module
                    })
                    .flat_map(|(_, candidate)| &candidate.function.clauses)
                    .filter_map(|clause| clause.body.core_expr.as_ref()),
            )?);
            managed_collections.extend(super::http_values::http_managed_collections(core)?);
            managed_collections.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
            managed_collections.dedup_by(|left, right| left.as_ref() == right.as_ref());
            modules.push(NativeModule {
                name: core.module.clone(),
                functions,
                continuations,
                managed_layouts,
                managed_collections,
                atoms: atoms.clone(),
            });
        }
    }
    if !lifted_functions.is_empty() {
        let mut managed_layouts = modules
            .iter()
            .flat_map(|module| module.managed_layouts.iter().cloned())
            .collect::<Vec<_>>();
        managed_layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        managed_layouts.dedup_by(|left, right| left.as_ref() == right.as_ref());
        let mut managed_collections = modules
            .iter()
            .flat_map(|module| module.managed_collections.iter().cloned())
            .collect::<Vec<_>>();
        managed_collections.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        managed_collections.dedup_by(|left, right| left.as_ref() == right.as_ref());
        modules.push(NativeModule {
            name: "$terlan.closures".to_string(),
            functions: lifted_functions,
            continuations: Vec::new(),
            managed_layouts,
            managed_collections,
            atoms: atoms.clone(),
        });
    }
    super::continuation_sharing::materialize_shared_continuations(&mut modules)?;
    Ok(modules)
}

fn application_composable_candidates(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
    suspending: &HashSet<usize>,
) -> HashSet<usize> {
    let mut composable_candidates = HashSet::new();
    loop {
        let before = composable_candidates.len();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            if !selected[candidate_index] || composable_candidates.contains(&candidate_index) {
                continue;
            }
            let resolver = &resolvers[&candidate.core.module];
            let suspending_names = resolved_names(resolver, suspending);
            let composable_names = resolver
                .iter()
                .filter(|(_, index)| composable_candidates.contains(*index))
                .map(|(identity, _)| identity.clone())
                .collect::<HashSet<_>>();
            let Some(body) = candidate
                .function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
            else {
                continue;
            };
            if is_composable_suspending_body(body, &suspending_names, &composable_names) {
                composable_candidates.insert(candidate_index);
            }
        }
        if composable_candidates.len() == before {
            return composable_candidates;
        }
    }
}

fn application_resolvers(
    cores: &[&CoreModule],
    candidates: &[Candidate<'_>],
    selected: &[bool],
) -> HashMap<String, HashMap<CallIdentity, usize>> {
    cores
        .iter()
        .map(|core| {
            let mut resolved = HashMap::<CallIdentity, Option<usize>>::new();
            let mut local_identities = HashSet::new();
            for (index, candidate) in candidates.iter().enumerate() {
                if !selected[index] || candidate.core.module != core.module {
                    continue;
                }
                let identity = (candidate.function.name.clone(), candidate.function.arity);
                local_identities.insert(identity.clone());
                resolved.insert(identity, Some(index));
            }
            for (index, candidate) in candidates.iter().enumerate() {
                if !selected[index]
                    || !candidate.function.public
                    || candidate.core.module == core.module
                    || !imports_function(core, candidate)
                {
                    continue;
                }
                for identity in [
                    (candidate.function.name.clone(), candidate.function.arity),
                    (
                        format!("{}.{}", candidate.core.module, candidate.function.name),
                        candidate.function.arity,
                    ),
                ] {
                    if local_identities.contains(&identity) {
                        continue;
                    }
                    match resolved.entry(identity) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(Some(index));
                        }
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            if entry.get().is_some_and(|existing| existing != index) {
                                entry.insert(None);
                            }
                        }
                    }
                }
            }
            let resolved = resolved
                .into_iter()
                .filter_map(|(identity, index)| index.map(|index| (identity, index)))
                .collect();
            (core.module.clone(), resolved)
        })
        .collect()
}

include!("application/remote_calls.rs");

fn imports_function(core: &CoreModule, candidate: &Candidate<'_>) -> bool {
    core.imports.iter().any(|import| {
        import.kind == CoreImportKind::Module
            && (import.module == candidate.core.module
                || import.module
                    == format!("{}.{}", candidate.core.module, candidate.function.name))
    })
}

fn application_suspending(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
) -> HashSet<usize> {
    let mut suspending = candidates
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            selected[*index]
                && candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                    .is_some_and(contains_process_yield)
        })
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    loop {
        let before = suspending.len();
        for (index, candidate) in candidates.iter().enumerate() {
            if !selected[index] || suspending.contains(&index) {
                continue;
            }
            let resolver = &resolvers[&candidate.core.module];
            if candidate
                .function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
                .is_some_and(|body| expr_calls_selected(body, resolver, &suspending))
            {
                suspending.insert(index);
            }
        }
        if suspending.len() == before {
            return suspending;
        }
    }
}

fn expr_calls_selected(
    expr: &CoreExpr,
    resolver: &HashMap<CallIdentity, usize>,
    selected: &HashSet<usize>,
) -> bool {
    match expr {
        CoreExpr::Call { function, args } => {
            resolver
                .get(&(function.clone(), args.len()))
                .is_some_and(|index| selected.contains(index))
                || args
                    .iter()
                    .any(|arg| expr_calls_selected(arg, resolver, selected))
        }
        // An owned closure can resolve to any admitted callable. Treat its
        // enclosing function as suspending so generated callers always carry
        // a transition buffer and can forward the selected target's result.
        CoreExpr::FunctionCall { .. } => true,
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .any(|arg| expr_calls_selected(arg, resolver, selected)),
        CoreExpr::UnaryOp { operand, .. } => expr_calls_selected(operand, resolver, selected),
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_calls_selected(left, resolver, selected)
                || expr_calls_selected(right, resolver, selected)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expr_calls_selected(&binding.value, resolver, selected))
                || expr_calls_selected(body, resolver, selected)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            expr_calls_selected(&clause.condition, resolver, selected)
                || expr_calls_selected(&clause.body, resolver, selected)
        }),
        _ => false,
    }
}

fn resolved_names(
    resolver: &HashMap<CallIdentity, usize>,
    selected: &HashSet<usize>,
) -> HashSet<CallIdentity> {
    resolver
        .iter()
        .filter(|(_, index)| selected.contains(index))
        .map(|(identity, _)| identity.clone())
        .collect()
}

fn native_resolver(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
) -> HashMap<CallIdentity, usize> {
    resolver
        .iter()
        .filter_map(|(identity, candidate)| {
            candidate_to_native
                .get(candidate)
                .map(|native| (identity.clone(), *native))
        })
        .collect()
}

fn native_function_types(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
    constructors: &super::constructors::NativeConstructorLayouts,
) -> HashMap<CallIdentity, NativeType> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            native_return_type_with_constructors(candidates[*candidate].function, constructors)
                .map(|native_type| (identity.clone(), native_type))
        })
        .collect()
}

fn native_function_core_types(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
) -> HashMap<CallIdentity, CoreType> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            candidates[*candidate]
                .function
                .core_return_type
                .clone()
                .map(|core_type| (identity.clone(), core_type))
        })
        .collect()
}

fn native_callable_shapes(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
    constructors: &super::constructors::NativeConstructorLayouts,
) -> HashMap<CallIdentity, NativeCallableShape> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            let candidate = candidates[*candidate];
            let parameters = candidate
                .function
                .params
                .iter()
                .map(|parameter| {
                    native_type_with_constructors(
                        parameter.core_ty.as_ref(),
                        &parameter.ty,
                        constructors,
                    )
                })
                .collect::<Option<Vec<_>>>()?;
            let result = native_return_type_with_constructors(candidate.function, constructors)?;
            Some((
                identity.clone(),
                NativeCallableShape {
                    id: super::stable_export_id(
                        &candidate.core.module,
                        &candidate.function.name,
                        candidate.function.arity,
                    ),
                    parameters,
                    result,
                },
            ))
        })
        .collect()
}
