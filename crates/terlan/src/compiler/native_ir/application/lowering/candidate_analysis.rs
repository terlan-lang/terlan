use super::*;

pub(super) struct CandidateAnalysis {
    pub(super) candidate_to_native: HashMap<usize, usize>,
    pub(super) native_function_labels: HashMap<usize, String>,
    pub(super) closure_factories: HashSet<usize>,
    pub(super) opaque_dynamic_boundaries: HashSet<usize>,
    pub(super) profile_dependencies: Vec<Vec<usize>>,
}

pub(super) fn analyze_candidates(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    candidate_resolvers: &HashMap<String, HashMap<CallIdentity, usize>>,
) -> CandidateAnalysis {
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
    let closure_factories = candidates
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            selected[*index]
                && candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                    .and_then(dynamic_targets::returned_lambda)
                    .is_some()
        })
        .map(|(index, _)| index)
        .collect::<HashSet<_>>();
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
    let profile_dependencies = candidates
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            if !selected[candidate_index] {
                return Vec::new();
            }
            let mut dependencies = Vec::new();
            if let Some(body) = candidate
                .function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
            {
                let resolver = &candidate_resolvers[&candidate.core.module];
                dynamic_targets::walk_calls(body, &mut |function, args| {
                    let identity = (function.to_string(), args.len());
                    if let Some(target) = resolver.get(&identity) {
                        if let Some(native) = candidate_to_native.get(target) {
                            dependencies.push(*native);
                        }
                    }
                });
            }
            dependencies.sort_unstable();
            dependencies.dedup();
            dependencies
        })
        .collect::<Vec<_>>();
    CandidateAnalysis {
        candidate_to_native,
        native_function_labels,
        closure_factories,
        opaque_dynamic_boundaries,
        profile_dependencies,
    }
}

pub(super) fn suspending_targets(
    suspending: &HashSet<usize>,
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
) -> Vec<String> {
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
    suspending_targets
}

pub(super) fn validate_profile_completeness(
    candidate_to_native: &HashMap<usize, usize>,
    candidates: &[Candidate<'_>],
    suspending: &HashSet<usize>,
    call_profiles: &HashMap<usize, crate::compiler::native_ir::ComposedCallProfile>,
    call_profile_gaps: &HashMap<usize, String>,
) -> Result<(), crate::compiler::native_ir::NativeIrError> {
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
    Ok(())
}

pub(super) struct RecursiveProfileSeeds {
    pub(super) recursive_components: Vec<Option<Vec<usize>>>,
    pub(super) call_profiles: HashMap<usize, crate::compiler::native_ir::ComposedCallProfile>,
}

pub(super) fn seed_recursive_profiles(
    candidates: &[Candidate<'_>],
    selected: &[bool],
    suspending: &HashSet<usize>,
    composable_candidates: &HashSet<usize>,
    candidate_to_native: &HashMap<usize, usize>,
    constructor_layouts: &HashMap<
        String,
        crate::compiler::native_ir::constructors::NativeConstructorLayouts,
    >,
    started: Instant,
) -> RecursiveProfileSeeds {
    let recursive_components = (0..candidates.len())
        .map(|candidate_index| recursive_reduction_component(candidate_index, candidates, selected))
        .collect::<Vec<_>>();
    trace_native_aot(
        started,
        "recursive-components",
        format_args!(
            "members={}",
            recursive_components
                .iter()
                .filter(|component| component.is_some())
                .count()
        ),
    );
    let call_profiles = candidates
        .iter()
        .enumerate()
        .filter_map(|(candidate_index, candidate)| {
            if !suspending.contains(&candidate_index) {
                return None;
            }
            let native_index = candidate_to_native.get(&candidate_index).copied()?;
            let profile = recursive_reduction_profile(
                candidate,
                recursive_components[candidate_index].as_deref(),
                candidates,
                &composable_candidates,
                &candidate_to_native,
                constructor_layouts,
            )?;
            Some((native_index, profile))
        })
        .collect::<HashMap<_, _>>();
    trace_native_aot(
        started,
        "recursive-profile-seeds",
        format_args!("profiles={}", call_profiles.len()),
    );
    RecursiveProfileSeeds {
        recursive_components,
        call_profiles,
    }
}
