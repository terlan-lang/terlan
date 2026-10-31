use super::*;

/// Completes fresh call-site variables from one unambiguous visible
/// conformance without specializing variables owned by the enclosing caller.
///
/// A generic helper can place some parameters only in its trait bound and
/// return type. For example, `Streaming[M, S, I, O, F]` may receive values of
/// `M`, `S`, and `I`, while `O` and `F` are determined by the selected
/// implementation. Ordinary argument unification cannot discover those two
/// types. This pass matches the partially resolved bound against visible impl
/// heads and adopts a refinement only when every matching impl yields the same
/// substitution for variables allocated by this call.
pub(crate) fn refine_call_local_trait_bounds(
    scheme: &FunctionScheme,
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
    call_local_var_floor: TypeVarId,
) {
    for bound in &scheme.bounds {
        if bound.trait_name == STRUCTURAL_IMPLICATION_BOUND {
            continue;
        }
        let Some(candidates) = ctx.trait_bound_impl_type_args.get(&bound.trait_name) else {
            continue;
        };
        let resolved = bound
            .trait_args
            .iter()
            .map(|arg| expand_type_aliases(&apply_subst(arg, subst), ctx.aliases))
            .collect::<Vec<_>>();
        let local_ids = bound
            .trait_args
            .iter()
            .filter_map(max_type_var)
            .max()
            .filter(|maximum| *maximum >= call_local_var_floor)
            .map(|maximum| (call_local_var_floor..=maximum).collect::<Vec<_>>())
            .unwrap_or_default();
        if local_ids.is_empty()
            || !local_ids
                .iter()
                .any(|id| apply_subst(&Type::Var(*id), subst) == Type::Var(*id))
        {
            continue;
        }

        let mut refinements = Vec::<(Vec<Type>, HashMap<TypeVarId, Type>)>::new();
        for candidate in candidates {
            if candidate.len() != resolved.len() {
                continue;
            }
            let mut next_var = resolved
                .iter()
                .chain(candidate)
                .filter_map(max_type_var)
                .max()
                .map_or(call_local_var_floor, |id| id + 1)
                .max(ctx.fresh_type_var_floor);
            let mut remap = HashMap::new();
            let normalized_candidate = candidate
                .iter()
                .map(|arg| {
                    let expanded = expand_type_aliases(arg, ctx.aliases);
                    remap_type_var_id(&expanded, &mut next_var, &mut remap)
                })
                .collect::<Vec<_>>();
            let mut trial = subst.clone();
            if resolved
                .iter()
                .zip(&normalized_candidate)
                .any(|(required, implementation)| {
                    unify(required, implementation, &mut trial).is_err()
                })
            {
                continue;
            }
            // A callee's fresh inference variables may be refined. A type
            // variable inherited from the caller remains universally
            // quantified and must never be fixed by a concrete impl.
            if trial
                .iter()
                .any(|(id, value)| *id < call_local_var_floor && subst.get(id) != Some(value))
            {
                continue;
            }
            let projection = local_ids
                .iter()
                .map(|id| apply_subst(&Type::Var(*id), &trial))
                .collect::<Vec<_>>();
            if projection
                .iter()
                .zip(&local_ids)
                .all(|(ty, id)| *ty == Type::Var(*id))
            {
                continue;
            }
            if !refinements
                .iter()
                .any(|(existing, _)| existing == &projection)
            {
                refinements.push((projection, trial));
            }
        }
        if let [(_, refinement)] = refinements.as_slice() {
            *subst = refinement.clone();
        }
    }
}
