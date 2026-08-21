use super::super::{call_composition::walk_native_expr, model::NativeExpr};
use super::*;
use std::time::Instant;

#[path = "lowering/module_assembly.rs"]
mod module_assembly;
#[path = "lowering/support.rs"]
mod support;
use module_assembly::{
    assemble_native_module, finalize_native_application, ApplicationFinalizationContext,
    ModuleAssemblyContext,
};
use support::{forwarded_dynamic_profile, profile_widths, trace_native_aot, widest_profile_labels};

pub(super) fn lower_selected_application(
    cores: &[&CoreModule],
    candidates: &[Candidate<'_>],
    selected: &[bool],
    constructor_layouts: &HashMap<String, super::super::constructors::NativeConstructorLayouts>,
) -> Result<Vec<NativeModule>, super::super::NativeIrError> {
    let started = Instant::now();
    let atoms = application_atom_identities(cores);
    let candidate_to_native = selected
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
        .enumerate()
        .map(|(native_index, (candidate_index, _))| (candidate_index, native_index))
        .collect::<HashMap<_, _>>();
    let native_function_labels = candidate_to_native
        .iter()
        .map(|(candidate_index, native_index)| {
            let candidate = &candidates[*candidate_index];
            (
                *native_index,
                format!(
                    "{}.{}/{}",
                    candidate.core.module, candidate.function.name, candidate.function.arity
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let candidate_resolvers = application_resolvers(cores, candidates, selected);
    let dynamic_parameter_targets =
        dynamic_targets::candidate_parameter_targets(candidates, selected, &candidate_resolvers);
    let mut internally_called = HashSet::new();
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
        dynamic_targets::walk_calls(body, &mut |function, args| {
            if let Some(target) =
                candidate_resolvers[&caller.core.module].get(&(function.to_string(), args.len()))
            {
                internally_called.insert(*target);
            }
        });
    }
    // A direct dynamic tail call that is reachable only as a public dispatch
    // boundary can forward the selected closure's opaque transition record to
    // the VM. It has no in-image caller that needs a statically composed
    // continuation graph, so requiring a fictitious closed-world target set
    // would reject the intended cross-image closure ABI.
    let opaque_dynamic_boundaries = candidates
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            selected[*index]
                && candidate.function.public
                && !internally_called.contains(index)
                && candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                    .is_some_and(|body| matches!(body, CoreExpr::FunctionCall { .. }))
        })
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
    let mut suspending = application_suspending(candidates, selected, &candidate_resolvers);
    suspending.retain(|candidate| !opaque_dynamic_boundaries.contains(candidate));
    let suspending_native = suspending
        .iter()
        .filter_map(|candidate| candidate_to_native.get(candidate).copied())
        .collect::<HashSet<_>>();
    let suspending_targets = suspending
        .iter()
        .filter_map(|candidate_index| {
            let native_index = candidate_to_native.get(candidate_index)?;
            let candidate = candidates.get(*candidate_index)?;
            Some(format!(
                "{native_index}={}.{}:{}",
                candidate.core.module, candidate.function.name, candidate.function.arity
            ))
        })
        .collect::<Vec<_>>();
    let composable_candidates =
        application_composable_candidates(candidates, selected, &candidate_resolvers, &suspending);
    trace_native_aot(
        started,
        "analysis",
        format_args!(
            "candidates={} suspending={} composable={}",
            candidate_to_native.len(),
            suspending.len(),
            composable_candidates.len()
        ),
    );
    let mut call_profiles = candidates
        .iter()
        .enumerate()
        .filter_map(|(candidate_index, candidate)| {
            let native_index = candidate_to_native.get(&candidate_index).copied()?;
            let profile = recursive_reduction_profile(
                candidate_index,
                candidate,
                candidates,
                selected,
                &composable_candidates,
                &candidate_to_native,
                constructor_layouts,
            )?;
            Some((native_index, profile))
        })
        .collect::<HashMap<_, _>>();
    let recursive_seed_profiles = call_profiles.keys().copied().collect::<HashSet<_>>();
    let recursive_seed_call_profiles = call_profiles.clone();
    let mut call_profile_gaps = HashMap::new();
    // A closure passed through a public AOT entry can name any admitted
    // callable with the same checked signature even when there is no internal
    // call site from which to infer that target. Seed those ABI-visible
    // targets as pure, then replace the seed with the converged suspension
    // profile below when the target can yield.
    let mut dynamic_call_profiles = HashMap::new();
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if !selected[candidate_index] {
            continue;
        }
        let constructors = &constructor_layouts[&candidate.core.module];
        let Some(parameters) = candidate
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
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(result) = native_return_type_with_constructors(candidate.function, constructors)
        else {
            continue;
        };
        merge_dynamic_call_profile(
            &mut dynamic_call_profiles,
            super::super::call_composition::DynamicCallSignature { parameters, result },
            super::super::stable_export_id(
                &candidate.core.module,
                &candidate.function.name,
                candidate.function.arity,
            ),
            format!(
                "{}.{}/{}",
                candidate.core.module, candidate.function.name, candidate.function.arity
            ),
            ComposedCallProfile::pure(),
        )?;
    }
    let mut dynamic_profile_gaps = HashMap::new();
    let mut refined_recursive_profiles = HashSet::new();
    let mut profile_lowerings = HashMap::new();
    // Recursive components start from their synthetic reduction profiles, then
    // every ordinary caller is refreshed until the complete profile graph is
    // unchanged. A fixed number of caller passes is incorrect: candidate
    // order is not dependency order, so an earlier caller can otherwise cache
    // the previous generation of a later callee's continuation identities.
    let maximum_profile_phases = suspending.len().saturating_add(4);
    let mut profiles_converged = false;
    for phase in 0..maximum_profile_phases {
        let stable_snapshot =
            (phase >= 3).then(|| (call_profiles.clone(), dynamic_call_profiles.clone()));
        if phase == 2 {
            refined_recursive_profiles.clear();
        }
        let mut phase_refreshed = HashSet::new();
        loop {
            let mut progress = false;
            for (candidate_index, candidate) in candidates.iter().enumerate() {
                if !selected[candidate_index] {
                    continue;
                }
                let native_index = candidate_to_native[&candidate_index];
                let recursive_seed = recursive_seed_profiles.contains(&native_index);
                let skip = match phase {
                    0 => {
                        call_profiles.contains_key(&native_index)
                            && (!recursive_seed
                                || refined_recursive_profiles.contains(&native_index))
                    }
                    1 => recursive_seed || phase_refreshed.contains(&native_index),
                    2 => !recursive_seed || refined_recursive_profiles.contains(&native_index),
                    3.. => phase_refreshed.contains(&native_index),
                };
                if skip {
                    continue;
                }
                let recursive_component_natives = recursive_seed
                    .then(|| recursive_reduction_component(candidate_index, candidates, selected))
                    .flatten()
                    .into_iter()
                    .flatten()
                    .filter_map(|member| candidate_to_native.get(&member).copied())
                    .collect::<Vec<_>>();
                let recursive_profile_inputs = recursive_seed.then(|| {
                    let mut inputs = call_profiles.clone();
                    for member_native in &recursive_component_natives {
                        if let Some(seed) = recursive_seed_call_profiles.get(member_native) {
                            inputs.insert(*member_native, seed.clone());
                        }
                    }
                    inputs
                });
                let profile_inputs = recursive_profile_inputs.as_ref().unwrap_or(&call_profiles);
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
                let mut profile_ids = HashSet::new();
                let mut profile_lifted = Vec::new();
                let candidate_dynamic_profiles = dynamic_targets::restrict_profiles(
                    &dynamic_call_profiles,
                    dynamic_parameter_targets.get(&candidate_index),
                );
                let lowered = super::super::lower_native_function_with_callables(
                    &candidate.core.module,
                    candidate.function,
                    super::super::NativeFunctionLoweringEnvironment {
                        identities: &identities,
                        function_types: &function_types,
                        function_core_types: &function_core_types,
                        callable_shapes: &callable_shapes,
                        constructors: &constructor_layouts[&candidate.core.module],
                        suspending_functions: &suspending_names,
                        call_profiles: profile_inputs,
                        dynamic_call_profiles: &candidate_dynamic_profiles,
                    },
                    super::super::NativeFunctionLoweringOutputs {
                        lifted_functions: &mut profile_lifted,
                        stable_ids: &mut profile_ids,
                    },
                );
                let (mut native, mut continuations) = match lowered {
                    Ok(lowered) => {
                        call_profile_gaps.remove(&native_index);
                        lowered
                    }
                    Err(error) => {
                        call_profile_gaps
                            .insert(native_index, format!("native lowering failed: {error}"));
                        if !recursive_seed {
                            progress |= call_profiles.remove(&native_index).is_some();
                            profile_lowerings.remove(&native_index);
                        }
                        phase_refreshed.insert(native_index);
                        continue;
                    }
                };
                let mut dynamic_progress = false;
                for lifted in &profile_lifted {
                    let profile =
                        ComposedCallProfile::new(&lifted.body, &continuations, profile_inputs)
                            .or_else(|| {
                                forwarded_dynamic_profile(&lifted.body, &candidate_dynamic_profiles)
                            })
                            .or_else(|| {
                                let super::super::NativeExpr::TailCall { function, .. } =
                                    &lifted.body
                                else {
                                    return None;
                                };
                                profile_inputs.get(function).cloned()
                            });
                    let profile = match profile {
                        Some(profile) => profile,
                        None if super::super::call_composition::is_definitely_non_suspending(
                            &lifted.body,
                            &suspending,
                        ) =>
                        {
                            ComposedCallProfile::pure()
                        }
                        None => {
                            dynamic_profile_gaps.insert(
                                lifted.export_id,
                                super::super::call_composition::profile_gap_reason(
                                    &lifted.body,
                                    &continuations,
                                    profile_inputs,
                                    &native_function_labels,
                                    &call_profile_gaps,
                                ),
                            );
                            continue;
                        }
                    };
                    let signature = super::super::call_composition::DynamicCallSignature {
                        parameters: lifted.params[lifted.callable_captures.len()..].to_vec(),
                        result: lifted.return_type,
                    };
                    dynamic_progress |= merge_dynamic_call_profile(
                        &mut dynamic_call_profiles,
                        signature,
                        lifted.export_id,
                        format!(
                            "{}.{}/{}",
                            lifted.source_module, lifted.source_function, lifted.source_arity
                        ),
                        profile,
                    )?;
                    dynamic_profile_gaps.remove(&lifted.export_id);
                }
                if !composable_candidates.contains(&candidate_index) {
                    let body = candidate
                        .function
                        .clauses
                        .first()
                        .and_then(|clause| clause.body.core_expr.as_ref());
                    let mut blocked = Vec::new();
                    if let Some(body) = body {
                        dynamic_targets::walk_calls(body, &mut |function, args| {
                            let identity = (function.to_string(), args.len());
                            let Some(target) =
                                candidate_resolvers[&candidate.core.module].get(&identity)
                            else {
                                return;
                            };
                            if suspending.contains(target)
                                && !composable_candidates.contains(target)
                            {
                                blocked.push(
                                    native_function_labels
                                        .get(&candidate_to_native[target])
                                        .cloned()
                                        .unwrap_or_else(|| format!("candidate {target}")),
                                );
                            }
                        });
                    }
                    blocked.sort();
                    blocked.dedup();
                    let reason = if blocked.is_empty() {
                        let composable_names = candidate_resolvers[&candidate.core.module]
                            .iter()
                            .filter(|(_, index)| composable_candidates.contains(*index))
                            .map(|(identity, _)| identity.clone())
                            .collect::<HashSet<_>>();
                        let admission_gap =
                            super::super::call_composition::composable_suspension_gap_reason(
                                body.expect("selected native candidate has a checked body"),
                                &suspending_names,
                                &composable_names,
                            );
                        let native_operation = candidate.function.native_operation.is_some();
                        let direct_yield = body.is_some_and(super::super::contains_process_yield);
                        let recursive_reduction = candidate
                            .core
                            .termination
                            .function(&candidate.function.name, candidate.function.arity)
                            .is_some_and(|evidence| {
                                evidence
                                    .recursive_calls
                                    .iter()
                                    .any(|edge| edge.tail_position)
                            });
                        format!(
                            "{admission_gap}; suspension seeds: native_operation={native_operation}, direct_yield={direct_yield}, recursive_reduction={recursive_reduction}"
                        )
                    } else {
                        format!(
                            "candidate depends on non-composable suspension targets [{}]",
                            blocked.join(", ")
                        )
                    };
                    call_profile_gaps.insert(native_index, reason);
                    if !recursive_seed {
                        progress |= call_profiles.remove(&native_index).is_some();
                        profile_lowerings.remove(&native_index);
                    }
                    phase_refreshed.insert(native_index);
                    progress |= dynamic_progress;
                    continue;
                }
                super::super::continuation_sharing::intern_function_continuations(
                    &mut native.body,
                    &mut continuations,
                );
                if super::super::has_uncomposed_suspending_call(&native.body, &suspending_native)
                    || continuations.iter().any(|continuation| {
                        super::super::has_uncomposed_suspending_call(
                            &continuation.body,
                            &suspending_native,
                        )
                    })
                {
                    let uncomposed_continuations = continuations
                        .iter()
                        .filter(|continuation| {
                            super::super::has_uncomposed_suspending_call(
                                &continuation.body,
                                &suspending_native,
                            )
                        })
                        .map(|continuation| format!("{}={:#?}", continuation.id, continuation.body))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut uncomposed_targets = Vec::new();
                    let mut record_target = |expr: &NativeExpr| {
                        let NativeExpr::Call { function, .. } = expr else {
                            return;
                        };
                        if suspending_native.contains(function) {
                            uncomposed_targets.push(
                                native_function_labels
                                    .get(function)
                                    .cloned()
                                    .unwrap_or_else(|| function.to_string()),
                            );
                        }
                    };
                    walk_native_expr(&native.body, &mut record_target);
                    for continuation in &continuations {
                        walk_native_expr(&continuation.body, &mut record_target);
                    }
                    uncomposed_targets.sort();
                    uncomposed_targets.dedup();
                    trace_native_aot(
                        started,
                        "profile-gap-uncomposed",
                        format_args!(
                            "function={} targets=[{}] body={:#?} uncomposed-continuations=[{}]",
                            native_function_labels
                                .get(&native_index)
                                .map(String::as_str)
                                .unwrap_or("<unknown>"),
                            uncomposed_targets.join(","),
                            native.body,
                            uncomposed_continuations
                        ),
                    );
                    call_profile_gaps.insert(
                        native_index,
                        "lowered body retains an uncomposed suspending call".to_string(),
                    );
                    if !recursive_seed {
                        progress |= call_profiles.remove(&native_index).is_some();
                        profile_lowerings.remove(&native_index);
                    }
                    phase_refreshed.insert(native_index);
                    continue;
                }
                let profile =
                    ComposedCallProfile::new(&native.body, &continuations, profile_inputs)
                        .or_else(|| {
                            forwarded_dynamic_profile(&native.body, &candidate_dynamic_profiles)
                        })
                        .or_else(|| {
                            let super::super::NativeExpr::TailCall { function, .. } = &native.body
                            else {
                                return None;
                            };
                            profile_inputs.get(function).cloned()
                        })
                        .or_else(|| {
                            super::super::call_composition::is_definitely_non_suspending(
                                &native.body,
                                &suspending,
                            )
                            .then(ComposedCallProfile::pure)
                        });
                if let Some(mut profile) = profile {
                    call_profile_gaps.remove(&native_index);
                    if recursive_seed {
                        for member in &recursive_component_natives {
                            if let Some(seed) = recursive_seed_call_profiles.get(member) {
                                profile.merge_recursive_component_profile(seed);
                            }
                            if let Some(refined) = call_profiles.get(member) {
                                profile.merge_recursive_component_profile(refined);
                            }
                        }
                        let entries = profile
                            .refresh_recursive_component_contract(&recursive_component_natives);
                        for member in &recursive_component_natives {
                            super::super::call_composition::refresh_recursive_call_contract(
                                &mut native.body,
                                *member,
                                &entries,
                            );
                            for continuation in &mut continuations {
                                super::super::call_composition::refresh_recursive_call_contract(
                                    &mut continuation.body,
                                    *member,
                                    &entries,
                                );
                            }
                        }
                    }
                    merge_dynamic_call_profile(
                        &mut dynamic_call_profiles,
                        super::super::call_composition::DynamicCallSignature {
                            parameters: native.params.clone(),
                            result: native.return_type,
                        },
                        native.export_id,
                        format!(
                            "{}.{}/{}",
                            candidate.core.module,
                            candidate.function.name,
                            candidate.function.arity
                        ),
                        profile.clone(),
                    )?;
                    call_profiles.insert(native_index, profile);
                    profile_lowerings.insert(
                        native_index,
                        (native, continuations, profile_lifted, profile_ids),
                    );
                    phase_refreshed.insert(native_index);
                    if recursive_seed_profiles.contains(&native_index) {
                        refined_recursive_profiles.insert(native_index);
                    }
                    progress = true;
                } else {
                    let reason = super::super::call_composition::profile_gap_reason(
                        &native.body,
                        &continuations,
                        profile_inputs,
                        &native_function_labels,
                        &call_profile_gaps,
                    );
                    call_profile_gaps.insert(native_index, reason);
                    if !recursive_seed {
                        progress |= call_profiles.remove(&native_index).is_some();
                        profile_lowerings.remove(&native_index);
                    }
                    phase_refreshed.insert(native_index);
                    if dynamic_progress {
                        progress = true;
                    }
                }
            }
            if !progress {
                break;
            }
        }
        if stable_snapshot.is_some_and(|snapshot| {
            snapshot == (call_profiles.clone(), dynamic_call_profiles.clone())
        }) {
            profiles_converged = true;
            let (total, maximum) = profile_widths(&call_profiles);
            trace_native_aot(
                started,
                "profiles-converged",
                format_args!(
                    "phase={phase} profiles={} total-continuations={total} maximum-width={maximum} widest=[{}]",
                    call_profiles.len(),
                    widest_profile_labels(&call_profiles, &native_function_labels)
                ),
            );
            break;
        }
        let (total, maximum) = profile_widths(&call_profiles);
        trace_native_aot(
            started,
            "profile-phase",
            format_args!(
                "phase={phase} profiles={} total-continuations={total} maximum-width={maximum}",
                call_profiles.len()
            ),
        );
    }
    if !profiles_converged {
        return Err(format!(
            "error[native_ir.call_profile_convergence]: suspension profiles did not converge after {maximum_profile_phases} bounded phases"
        )
        .into());
    }
    let mut missing_profiles = candidate_to_native
        .iter()
        .filter(|(candidate, native)| {
            suspending.contains(candidate) && !call_profiles.contains_key(native)
        })
        .map(|(candidate_index, native_index)| {
            let candidate = &candidates[*candidate_index];
            let reason = call_profile_gaps
                .get(native_index)
                .map(String::as_str)
                .unwrap_or("no suspension profile was produced");
            format!(
                "{}.{}/{}: {reason}",
                candidate.core.module, candidate.function.name, candidate.function.arity
            )
        })
        .collect::<Vec<_>>();
    missing_profiles.sort();
    if !missing_profiles.is_empty() {
        return Err(format!(
            "error[native_ir.call_profile_missing]: suspending functions have no converged profiles: [{}]",
            missing_profiles.join("; ")
        )
        .into());
    }
    let mut profile_owners = call_profiles.keys().copied().collect::<Vec<_>>();
    profile_owners.sort_unstable();
    let profile_destination_capture_counts = call_profiles
        .values()
        .flat_map(|profile| {
            profile
                .continuations
                .iter()
                .map(|continuation| (continuation.id, continuation.params.len()))
        })
        .collect::<HashMap<_, _>>();
    let mut validated_profile_continuations = HashSet::new();
    trace_native_aot(
        started,
        "profile-contracts-start",
        format_args!("profiles={}", profile_owners.len()),
    );
    for owner in profile_owners {
        let profile = &call_profiles[&owner];
        for continuation in &profile.continuations {
            if !validated_profile_continuations.insert(continuation.id) {
                continue;
            }
            super::super::call_composition::validate_call_then_contracts_with_destinations(
                &continuation.body,
                &call_profiles,
                &native_function_labels,
                &profile_destination_capture_counts,
            )
            .map_err(|error| {
                let owner = native_function_labels
                    .get(&owner)
                    .map_or_else(|| owner.to_string(), Clone::clone);
                format!(
                    "error[native_ir.call_profile_contract]: {error}; in continuation {} of `{owner}`",
                    continuation.id
                )
            })?;
        }
    }
    trace_native_aot(
        started,
        "profile-contracts-complete",
        format_args!(
            "unique-continuations={}",
            validated_profile_continuations.len()
        ),
    );
    let mut export_ids = HashSet::new();
    let mut modules = Vec::new();
    let mut lifted_functions = Vec::new();
    for core in cores {
        trace_native_aot(started, "module-start", &core.module);
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
            let native_index = candidate_to_native[&candidate_index];
            dynamic_targets::validate_profiles(
                &dynamic_call_profiles,
                dynamic_parameter_targets.get(&candidate_index),
                &format!(
                    "{}.{}/{}",
                    candidate.core.module, candidate.function.name, candidate.function.arity
                ),
                &dynamic_profile_gaps,
            )?;
            let candidate_dynamic_profiles = dynamic_targets::restrict_profiles(
                &dynamic_call_profiles,
                dynamic_parameter_targets.get(&candidate_index),
            );
            // The converged fixed-point pass already lowered every suspending
            // function against the final profile graph and interned its
            // continuations. Re-lowering those functions here duplicated the
            // complete wrapper closure and made large repository tools pay an
            // exponential-looking emission cost after convergence. Reuse the
            // exact lowering that produced the admitted profile; pure
            // functions, which have no profile entry, are still lowered once
            // during final emission.
            let cached_lowering = profile_lowerings.get(&native_index);
            let reused_converged_lowering = cached_lowering.is_some();
            let (mut function, mut function_continuations) = if let Some((
                function,
                continuations,
                cached_lifted,
                cached_ids,
            )) = cached_lowering
            {
                for id in cached_ids {
                    if !export_ids.insert(*id) {
                        return Err(format!(
                            "error[native_ir.export_identity]: cached lowering for `{}.{}/{}` duplicates export identity {id}",
                            core.module, candidate.function.name, candidate.function.arity
                        )
                        .into());
                    }
                }
                lifted_functions.extend(cached_lifted.iter().cloned());
                (function.clone(), continuations.clone())
            } else {
                super::super::lower_native_function_with_callables(
                    &core.module,
                    candidate.function,
                    super::super::NativeFunctionLoweringEnvironment {
                        identities: &identities,
                        function_types: &function_types,
                        function_core_types: &function_core_types,
                        callable_shapes: &callable_shapes,
                        constructors: &constructor_layouts[&core.module],
                        suspending_functions: &suspending_names,
                        call_profiles: &call_profiles,
                        dynamic_call_profiles: &candidate_dynamic_profiles,
                    },
                    super::super::NativeFunctionLoweringOutputs {
                        lifted_functions: &mut lifted_functions,
                        stable_ids: &mut export_ids,
                    },
                )
                .map_err(|error| {
                    format!(
                        "{error}; while lowering `{}.{}/{}`",
                        core.module, candidate.function.name, candidate.function.arity
                    )
                })?
            };
            if !reused_converged_lowering {
                super::super::continuation_sharing::intern_function_continuations(
                    &mut function.body,
                    &mut function_continuations,
                );
            }
            if !reused_converged_lowering && !recursive_seed_profiles.contains(&native_index) {
                if let Some(expected) = call_profiles.get(&native_index) {
                    let emitted = ComposedCallProfile::new(
                        &function.body,
                        &function_continuations,
                        &call_profiles,
                    )
                    .or_else(|| {
                        forwarded_dynamic_profile(&function.body, &candidate_dynamic_profiles)
                    })
                    .or_else(|| {
                        let super::super::NativeExpr::TailCall {
                            function: target, ..
                        } = &function.body
                        else {
                            return None;
                        };
                        call_profiles.get(target).cloned()
                    })
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.profile_emission]: final lowering for `{}.{}/{}` has no suspension profile",
                            core.module, candidate.function.name, candidate.function.arity
                        )
                    })?;
                    if &emitted != expected {
                        let expected_ids = expected
                            .continuations
                            .iter()
                            .map(|continuation| continuation.id)
                            .collect::<HashSet<_>>();
                        let emitted_ids = emitted
                            .continuations
                            .iter()
                            .map(|continuation| continuation.id)
                            .collect::<HashSet<_>>();
                        let mut missing = expected_ids
                            .difference(&emitted_ids)
                            .copied()
                            .collect::<Vec<_>>();
                        let mut extra = emitted_ids
                            .difference(&expected_ids)
                            .copied()
                            .collect::<Vec<_>>();
                        missing.sort_unstable();
                        extra.sort_unstable();
                        return Err(format!(
                            "error[native_ir.profile_emission]: final lowering for `{}.{}/{}` differs from its converged suspension profile; missing={missing:?}, extra={extra:?}",
                            core.module, candidate.function.name, candidate.function.arity
                        )
                        .into());
                    }
                }
            }
            functions.push(function);
            if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some()
                && function_continuations.len() >= 100
            {
                trace_native_aot(
                    started,
                    "function-emitted",
                    format_args!(
                        "function={}.{}:{} continuations={}",
                        core.module,
                        candidate.function.name,
                        candidate.function.arity,
                        function_continuations.len()
                    ),
                );
            }
            continuations.append(&mut function_continuations);
        }
        if let Some(module) = assemble_native_module(
            ModuleAssemblyContext {
                started,
                core,
                candidates,
                selected,
                constructor_layouts,
                atoms: &atoms,
            },
            functions,
            continuations,
        )? {
            modules.push(module);
            trace_native_aot(
                started,
                "module-lowered",
                format_args!("module={} modules={}", core.module, modules.len()),
            );
        }
    }
    finalize_native_application(
        ApplicationFinalizationContext {
            started,
            atoms: &atoms,
            call_profiles: &call_profiles,
            function_labels: &native_function_labels,
            suspending_native: &suspending_native,
            suspending_targets: &suspending_targets,
        },
        modules,
        lifted_functions,
    )
}
