//! Application-wide NativeIR admission and symbol resolution.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreImportKind, CoreModule, CorePattern, CoreType,
};

use super::{
    aggregate_types::managed_aggregate_layouts, atom_inventory::application_atom_identities,
    collections::managed_collection_layouts, composable_suspending_functions,
    constructors::native_constructor_layouts, contains_process_yield, expr_calls_are_supported,
    is_scalar_candidate, native_return_type, native_type,
    scalar_replacement::scalar_replace_fixed_aggregates, ComposedCallProfile, NativeCallableShape,
    NativeContinuation, NativeModule, NativeType,
};

type CallIdentity = (String, usize);

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
        let mut specialization_budget =
            super::specialization_budget::SpecializationBudget::default();
        for core in &mut normalized_cores {
            super::constructor_chain::lower_constructor_chains(core);
            super::list_comprehension::lower_list_comprehensions(core)?;
            super::template_values::lower_template_values(core)?;
            super::http_values::lower_http_values(core)?;
            normalize_remote_calls(core);
            super::case_lowering::lower_scalar_cases(core)?;
            super::generic_specialization::specialize_private_generics_with_budget(
                core,
                &mut specialization_budget,
            )?;
            super::higher_order_specialization::specialize_higher_order_helpers_with_budget(
                core,
                &mut specialization_budget,
            )?;
            normalize_static_callables(core, &mut specialization_budget)?;
            normalize_dynamic_callable_aliases(core);
        }
        super::callee_scalar_replacement::specialize_projection_callees_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        let ordered_cores = normalized_cores.iter().collect::<Vec<_>>();
        let constructor_modules = ordered_cores
            .iter()
            .map(|core| (core.module.as_str(), core.constructors.as_slice()))
            .collect::<Vec<_>>();
        let mut constructor_layouts = ordered_cores
            .iter()
            .map(|core| {
                native_constructor_layouts(&constructor_modules, &core.module)
                    .map(|layouts| (core.module.clone(), layouts))
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
            return Err(format!(
                "error[native_ir.unsupported_application_function]: `{module}.{}/{}` cannot be lowered into the native application image; runtime CoreIR interpretation has been removed",
                function.name, function.arity
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
                let local_functions = candidates
                    .iter()
                    .enumerate()
                    .filter(|(candidate_index, item)| {
                        selected[*candidate_index] && item.core.module == candidate.core.module
                    })
                    .map(|(_, item)| item.function)
                    .collect::<Vec<_>>();
                let composable =
                    composable_suspending_functions(&local_functions, &suspending_names);
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
    let mut call_profiles = HashMap::new();

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if !selected[candidate_index] {
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
        );
        let callable_shapes = native_callable_shapes(
            &candidate_resolvers[&candidate.core.module],
            &candidate_to_native,
            candidates,
        );
        let suspending_names =
            resolved_names(&candidate_resolvers[&candidate.core.module], &suspending);
        let local_functions = candidates
            .iter()
            .enumerate()
            .filter(|(index, item)| selected[*index] && item.core.module == candidate.core.module)
            .map(|(_, item)| item.function)
            .collect::<Vec<_>>();
        let composable = composable_suspending_functions(&local_functions, &suspending_names);
        let identity = (candidate.function.name.clone(), candidate.function.arity);
        if !composable.contains(&identity) {
            continue;
        }
        let mut profile_ids = HashSet::new();
        let mut profile_lifted = Vec::new();
        let (native, continuations) = super::lower_native_function_with_callables(
            &candidate.core.module,
            candidate.function,
            &identities,
            &function_types,
            &callable_shapes,
            &mut profile_lifted,
            &constructor_layouts[&candidate.core.module],
            &suspending_names,
            &HashMap::new(),
            &mut profile_ids,
        )?;
        if let Some(profile) = ComposedCallProfile::new(&native.body, &continuations) {
            call_profiles.insert(candidate_to_native[&candidate_index], profile);
        }
    }

    let mut export_ids = HashSet::new();
    let mut modules = Vec::new();
    let mut lifted_functions = Vec::new();
    for core in cores {
        let resolver = &candidate_resolvers[&core.module];
        let identities = native_resolver(resolver, &candidate_to_native);
        let function_types = native_function_types(resolver, &candidate_to_native, candidates);
        let callable_shapes = native_callable_shapes(resolver, &candidate_to_native, candidates);
        let suspending_names = resolved_names(resolver, &suspending);
        let mut functions = Vec::new();
        let mut continuations = Vec::<NativeContinuation>::new();
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            if !selected[candidate_index] || candidate.core.module != core.module {
                continue;
            }
            let (function, mut function_continuations) =
                super::lower_native_function_with_callables(
                    &core.module,
                    candidate.function,
                    &identities,
                    &function_types,
                    &callable_shapes,
                    &mut lifted_functions,
                    &constructor_layouts[&core.module],
                    &suspending_names,
                    &call_profiles,
                    &mut export_ids,
                )?;
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
            managed_layouts.extend(super::http_values::http_managed_layouts(core)?);
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
    Ok(modules)
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

fn normalize_remote_calls(core: &mut CoreModule) {
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = &mut clause.body.core_expr {
                normalize_remote_expr(body);
            }
        }
    }
}

fn normalize_remote_expr(expr: &mut CoreExpr) {
    match expr {
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            for arg in args.iter_mut() {
                normalize_remote_expr(arg);
            }
            if super::http_values::is_managed_http_module(module)
                || super::template_values::is_managed_template_module(module)
                || super::list_comprehension::is_managed_comprehension_module(module)
            {
                return;
            }
            *expr = CoreExpr::Call {
                function: format!("{module}.{function}"),
                args: std::mem::take(args),
            };
        }
        CoreExpr::Call { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                normalize_remote_expr(arg);
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                normalize_remote_expr(arg);
            }
        }
        CoreExpr::UnaryOp { operand, .. } => normalize_remote_expr(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            normalize_remote_expr(left);
            normalize_remote_expr(right);
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                normalize_remote_expr(&mut binding.value);
            }
            normalize_remote_expr(body);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                normalize_remote_expr(&mut clause.condition);
                normalize_remote_expr(&mut clause.body);
            }
        }
        _ => {}
    }
}

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
) -> HashMap<CallIdentity, NativeType> {
    resolver
        .iter()
        .filter(|(_, candidate)| candidate_to_native.contains_key(candidate))
        .filter_map(|(identity, candidate)| {
            native_return_type(candidates[*candidate].function)
                .map(|native_type| (identity.clone(), native_type))
        })
        .collect()
}

fn native_callable_shapes(
    resolver: &HashMap<CallIdentity, usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
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
                .map(|parameter| native_type(parameter.core_ty.as_ref(), &parameter.ty))
                .collect::<Option<Vec<_>>>()?;
            let result = native_return_type(candidate.function)?;
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
