//! Application-wide suspension and composition analysis.

use super::*;

pub(super) fn merge_managed_layouts(
    layouts: &mut Vec<Arc<[u8]>>,
    additions: Vec<Arc<[u8]>>,
) -> Result<(), super::super::NativeIrError> {
    for addition in additions {
        let candidate = crate::runtime::native_image::managed::decode_aggregate_layout(&addition)
            .map_err(|error| format!("error[native_ir.managed_layout]: {error}"))?;
        let existing = layouts.iter().find_map(|encoded| {
            let descriptor =
                crate::runtime::native_image::managed::decode_aggregate_layout(encoded).ok()?;
            (descriptor.managed().semantic_id() == candidate.managed().semantic_id()
                && descriptor.kind() == candidate.kind()
                && descriptor.variant_name() == candidate.variant_name()
                && descriptor.discriminant() == candidate.discriminant())
            .then_some(descriptor)
        });
        if let Some(existing) = existing {
            let existing_fields = existing
                .fields()
                .iter()
                .map(|field| field.field_type())
                .collect::<Vec<_>>();
            let candidate_fields = candidate
                .fields()
                .iter()
                .map(|field| field.field_type())
                .collect::<Vec<_>>();
            if existing.variant_count() != candidate.variant_count()
                || existing_fields != candidate_fields
            {
                return Err(format!(
                    "error[native_ir.managed_layout_conflict]: semantic `{}` variant {:?} has incompatible physical layouts",
                    candidate.canonical_type(),
                    candidate.variant_name()
                )
                .into());
            }
            continue;
        }
        layouts.push(addition);
    }
    Ok(())
}

pub(super) fn merge_dynamic_call_profile(
    profiles: &mut super::super::call_composition::DynamicCallProfiles,
    signature: super::super::call_composition::DynamicCallSignature,
    export_id: u64,
    source: String,
    incoming: ComposedCallProfile,
) -> Result<bool, super::super::NativeIrError> {
    let targets = profiles.entry(signature).or_default();
    if let Some(existing) = targets
        .iter_mut()
        .find(|target| target.export_id == export_id)
    {
        if existing.profile == incoming {
            return Ok(false);
        }
        existing.profile = incoming;
        return Ok(true);
    }
    targets.push(super::super::call_composition::DynamicTargetProfile {
        export_id,
        source,
        profile: incoming,
    });
    targets.sort_by_key(|target| target.export_id);
    Ok(true)
}

pub(super) fn application_composable_candidates(
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
        admit_recursive_reduction_components(
            candidates,
            selected,
            resolvers,
            suspending,
            &mut composable_candidates,
        );
        if composable_candidates.len() == before {
            return composable_candidates;
        }
    }
}

pub(super) fn recursive_reduction_component(
    candidate_index: usize,
    candidates: &[Candidate<'_>],
    selected: &[bool],
) -> Option<Vec<usize>> {
    let candidate = candidates.get(candidate_index)?;
    let evidence = candidate
        .core
        .termination
        .function(&candidate.function.name, candidate.function.arity);
    if candidate.function.name.starts_with("$aot_generic_") {
        let body = candidate
            .function
            .clauses
            .first()
            .and_then(|clause| clause.body.core_expr.as_ref())?;
        let mut self_recursive = false;
        let source_name = candidate
            .function
            .name
            .strip_prefix("$aot_generic_")
            .and_then(|name| name.rsplit_once('_').map(|(source, _)| source));
        super::dynamic_targets::walk_calls(body, &mut |function, args| {
            if (function == candidate.function.name || source_name == Some(function))
                && args.len() == candidate.function.arity
            {
                self_recursive = true;
            }
        });
        if self_recursive && selected.get(candidate_index) == Some(&true) {
            return Some(vec![candidate_index]);
        }
    }
    let evidence = evidence?;
    if evidence.component.is_empty()
        || evidence.recursive_calls.is_empty()
        || evidence
            .recursive_calls
            .iter()
            .all(|edge| !edge.tail_position)
    {
        return None;
    }
    let component_names = evidence.component.iter().cloned().collect::<HashSet<_>>();
    let mut members = candidates
        .iter()
        .enumerate()
        .filter(|(index, member)| {
            selected[*index]
                && member.core.module == candidate.core.module
                && component_names.contains(&format!(
                    "{}/{}",
                    member.function.name, member.function.arity
                ))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    members.sort_unstable();
    members.dedup();
    (members.len() == component_names.len()).then_some(members)
}

pub(super) fn admit_recursive_reduction_components(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
    suspending: &HashSet<usize>,
    composable: &mut HashSet<usize>,
) {
    for candidate_index in 0..candidates.len() {
        let Some(members) = recursive_reduction_component(candidate_index, candidates, selected)
        else {
            continue;
        };
        if members
            .first()
            .is_some_and(|first| *first != candidate_index)
        {
            continue;
        }
        let admitted = members.iter().all(|member_index| {
            let member = &candidates[*member_index];
            let Some(body) = member
                .function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
            else {
                return false;
            };
            let resolver = &resolvers[&member.core.module];
            let suspending_names = resolved_names(resolver, suspending);
            let mut composable_names = resolver
                .iter()
                .filter(|(_, index)| composable.contains(*index) || members.contains(index))
                .map(|(identity, _)| identity.clone())
                .collect::<HashSet<_>>();
            composable_names.extend(members.iter().map(|index| {
                let function = &candidates[*index].function;
                (function.name.clone(), function.arity)
            }));
            is_composable_suspending_body(body, &suspending_names, &composable_names)
        });
        if admitted {
            composable.extend(members);
        }
    }
}

pub(super) fn recursive_reduction_profile(
    candidate_index: usize,
    candidate: &Candidate<'_>,
    candidates: &[Candidate<'_>],
    selected: &[bool],
    composable: &HashSet<usize>,
    candidate_to_native: &HashMap<usize, usize>,
    constructor_layouts: &HashMap<String, super::super::constructors::NativeConstructorLayouts>,
) -> Option<ComposedCallProfile> {
    let component = recursive_reduction_component(candidate_index, candidates, selected)?;
    if component.iter().any(|member| !composable.contains(member)) {
        return None;
    }
    let members = component
        .iter()
        .map(|member_index| {
            let member = candidates.get(*member_index)?;
            let constructors = constructor_layouts.get(&member.core.module)?;
            let params = member
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
            Some(RecursiveReductionMember {
                module: member.core.module.clone(),
                function_name: member.function.name.clone(),
                arity: member.function.arity,
                function: *candidate_to_native.get(member_index)?,
                params,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let result = native_return_type_with_constructors(
        candidate.function,
        constructor_layouts.get(&candidate.core.module)?,
    )?;
    Some(ComposedCallProfile::recursive_component(
        &candidate.core.module,
        &candidate.function.name,
        candidate.function.arity,
        result,
        members,
    ))
}

pub(super) fn validate_composed_suspending_calls(
    modules: &[NativeModule],
    suspending: &HashSet<usize>,
    targets: &[String],
) -> Result<(), super::super::NativeIrError> {
    for module in modules {
        for function in &module.functions {
            if super::super::has_uncomposed_suspending_call(&function.body, suspending) {
                return Err(format!(
                    "error[native_ir.uncomposed_suspending_call]: `{}.{}/{}` retains an ordinary call to a suspension-capable function; targets [{}]: {:#?}",
                    function.source_module,
                    function.source_function,
                    function.source_arity,
                    targets.join(", "),
                    function.body,
                )
                .into());
            }
        }
        for continuation in &module.continuations {
            if super::super::has_uncomposed_suspending_call(&continuation.body, suspending) {
                return Err(format!(
                    "error[native_ir.uncomposed_suspending_call]: continuation {} for `{}.{}/{}` retains an ordinary call to a suspension-capable function; targets [{}]: {:#?}",
                    continuation.id,
                    continuation.source_module,
                    continuation.source_function,
                    continuation.source_arity,
                    targets.join(", "),
                    continuation.body,
                )
                .into());
            }
        }
    }
    Ok(())
}

pub(super) fn application_resolvers(
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
                {
                    continue;
                }
                let mut identities = vec![(
                    format!("{}.{}", candidate.core.module, candidate.function.name),
                    candidate.function.arity,
                )];
                if imports_function(core, candidate) {
                    identities.push((candidate.function.name.clone(), candidate.function.arity));
                }
                for identity in identities {
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

pub(super) fn imports_function(core: &CoreModule, candidate: &Candidate<'_>) -> bool {
    core.imports.iter().any(|import| {
        import.kind == CoreImportKind::Module
            && (import.module == candidate.core.module
                || import.module
                    == format!("{}.{}", candidate.core.module, candidate.function.name))
    })
}

pub(super) fn application_suspending(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
) -> HashSet<usize> {
    let mut suspending = candidates
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            selected[*index]
                && (candidate.function.native_operation.is_some()
                    || candidate
                        .function
                        .clauses
                        .first()
                        .and_then(|clause| clause.body.core_expr.as_ref())
                        .is_some_and(contains_process_yield)
                    || candidate
                        .core
                        .termination
                        .function(&candidate.function.name, candidate.function.arity)
                        .is_some_and(|evidence| {
                            evidence
                                .recursive_calls
                                .iter()
                                .any(|edge| edge.tail_position)
                        }))
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

pub(super) fn expr_calls_selected(
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
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => items
            .iter()
            .any(|item| expr_calls_selected(item, resolver, selected)),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            expr_calls_selected(head, resolver, selected)
                || expr_calls_selected(tail, resolver, selected)
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            expr_calls_selected(expr, resolver, selected)
                || generators
                    .iter()
                    .any(|generator| expr_calls_selected(&generator.source, resolver, selected))
                || guards
                    .iter()
                    .any(|guard| expr_calls_selected(guard, resolver, selected))
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .any(|field| expr_calls_selected(&field.value, resolver, selected)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .any(|field| expr_calls_selected(&field.value, resolver, selected))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_calls_selected(base, resolver, selected)
                || fields
                    .iter()
                    .any(|field| expr_calls_selected(&field.value, resolver, selected))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_selected(base, resolver, selected)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter()
                .any(|arg| expr_calls_selected(arg, resolver, selected))
                || expr_calls_selected(record, resolver, selected)
        }
        CoreExpr::RemoteCall { args, .. } => args
            .iter()
            .any(|arg| expr_calls_selected(arg, resolver, selected)),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            expr_calls_selected(receiver, resolver, selected)
                || args
                    .iter()
                    .any(|arg| expr_calls_selected(arg, resolver, selected))
        }
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => args
            .iter()
            .any(|arg| expr_calls_selected(arg, resolver, selected)),
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            expr_calls_selected(operand, resolver, selected)
        }
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
        CoreExpr::Case { scrutinee, clauses } => {
            expr_calls_selected(scrutinee, resolver, selected)
                || clauses.iter().any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_selected(guard, resolver, selected))
                        || expr_calls_selected(&clause.body, resolver, selected)
                })
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            expr_calls_selected(body, resolver, selected)
                || of_clauses.iter().chain(catch_clauses).any(|clause| {
                    clause
                        .guard
                        .as_ref()
                        .is_some_and(|guard| expr_calls_selected(guard, resolver, selected))
                        || expr_calls_selected(&clause.body, resolver, selected)
                })
                || after_clause.as_ref().is_some_and(|after| {
                    expr_calls_selected(&after.trigger, resolver, selected)
                        || expr_calls_selected(&after.body, resolver, selected)
                })
        }
        // Creating a closure does not execute its body. The lifted callable is
        // classified independently when closure conversion lowers it.
        CoreExpr::Lam { .. } => false,
        CoreExpr::SqlQuery { parameters, .. } => parameters
            .iter()
            .any(|parameter| expr_calls_selected(parameter, resolver, selected)),
        _ => false,
    }
}

pub(super) fn resolved_names(
    resolver: &HashMap<CallIdentity, usize>,
    selected: &HashSet<usize>,
) -> HashSet<CallIdentity> {
    resolver
        .iter()
        .filter(|(_, index)| selected.contains(index))
        .map(|(identity, _)| identity.clone())
        .collect()
}
