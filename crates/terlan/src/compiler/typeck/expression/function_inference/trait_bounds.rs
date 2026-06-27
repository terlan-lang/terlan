use super::*;

/// Cache for repeated trait lookup work during expression inference.
///
/// Inputs:
/// - Trait bound checks and method lookup requests encountered while checking
///   one module.
///
/// Output:
/// - Memoized lookup results reused by expression inference.
///
/// Transformation:
/// - Avoids recomputing trait conformance and method dispatch searches while
///   keeping cache scope local to one typecheck pass.
#[derive(Debug, Default)]
pub(crate) struct TraitLookupCache {
    bound_checks: HashMap<TraitBoundLookupKey, bool>,
    pub(crate) method_calls: HashMap<TraitMethodLookupKey, TraitMethodLookupResult>,
}

/// Cache key for a trait bound conformance lookup.
///
/// Inputs:
/// - Trait name and concrete bound type arguments.
///
/// Output:
/// - Hashable key for `TraitLookupCache`.
///
/// Transformation:
/// - Normalizes the lookup request into owned type data so repeated bound
///   checks can share the same memoized result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitBoundLookupKey {
    trait_name: String,
    bound_args: Vec<Type>,
}

/// Cache key for a trait method dispatch lookup.
///
/// Inputs:
/// - Trait name, method name, and concrete call argument types.
///
/// Output:
/// - Hashable key for trait method call lookup results.
///
/// Transformation:
/// - Records the full method dispatch request so repeated calls can reuse the
///   previous ambiguity/single-candidate result.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TraitMethodLookupKey {
    pub(crate) trait_name: String,
    pub(crate) method_name: String,
    pub(crate) arg_types: Vec<Type>,
}

/// Cached trait method lookup result.
///
/// Inputs:
/// - Candidate trait methods and call argument types.
///
/// Output:
/// - No match, ambiguous match, or a single selected candidate index.
///
/// Transformation:
/// - Stores dispatch outcome without keeping borrowed candidate data in the
///   cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TraitMethodLookupResult {
    NoMatch,
    Ambiguous,
    Single(usize),
}

/// Finds a transparent alias name for a concrete type.
///
/// Inputs:
/// - `ty`: type to match.
/// - `aliases`: visible transparent aliases.
///
/// Output:
/// - Alias name whose expanded representation equals `ty`.
///
/// Transformation:
/// - Expands zero-parameter aliases and compares their pretty-printed
///   representation to the target type.
pub(crate) fn alias_name_for_type(
    ty: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> Option<String> {
    let rendered = pretty_type(ty);
    aliases.iter().find_map(|(name, alias)| {
        if !alias.params.is_empty() {
            return None;
        }
        if pretty_type(&expand_type_aliases(&alias.body, aliases)) == rendered {
            Some(name.clone())
        } else {
            None
        }
    })
}

/// Checks instantiated function trait bounds.
///
/// Inputs:
/// - `scheme`: instantiated function scheme with bounds.
/// - `function_name`: optional call-site diagnostic context.
/// - `ctx`: expression context with trait impl visibility.
/// - `subst`: current type substitutions.
///
/// Output:
/// - `Ok(())` when all bounds are satisfied, otherwise a diagnostic string.
///
/// Transformation:
/// - Resolves bound arguments through substitutions and alias expansion, then
///   checks visible impls and active callable bounds.
pub(crate) fn check_function_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if scheme.bounds.is_empty() {
        return Ok(());
    }

    for bound in &scheme.bounds {
        let resolved_args = bound
            .trait_args
            .iter()
            .map(|arg| {
                let arg = apply_subst(arg, subst);
                expand_type_aliases(&arg, ctx.aliases)
            })
            .collect::<Vec<_>>();
        let resolved_args = canonicalize_trait_lookup_types(&resolved_args);

        if !trait_has_bound_implementation(&bound.trait_name, &resolved_args, ctx) {
            let trait_description = if resolved_args.is_empty() {
                bound.trait_name.clone()
            } else {
                format!(
                    "{}[{}]",
                    bound.trait_name,
                    resolved_args
                        .iter()
                        .map(pretty_type)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            };

            let context = function_name.unwrap_or("expression");
            return Err(format!(
                "at `{}` call site: expected trait bound `{}`",
                context, trait_description
            ));
        }
    }

    Ok(())
}

/// Infers a trait method call using the active callable's generic bounds.
///
/// Inputs:
/// - `trait_name`: scoped trait name used at the call site.
/// - `method_name`: trait method name used at the call site.
/// - `arg_types`: already-inferred argument types at the call site.
/// - `ctx`: expression inference context with visible trait signatures and
///   active callable bounds.
/// - `subst`: mutable type substitution accumulated by the enclosing
///   expression inference.
///
/// Output:
/// - `Some(return_type)` when an active bound such as `Eq[A]` satisfies
///   `Eq.equal(...)` and the trait method signature type-checks with the
///   provided arguments.
/// - `None` when no active bound applies or the signature does not match.
///
/// Transformation:
/// - Specializes the trait method signature through the active bound's trait
///   arguments, then runs ordinary function-call inference against that
///   specialized signature. This does not synthesize a global impl candidate,
///   so concrete calls without an impl still produce the normal missing-impl
///   diagnostic.
pub(crate) fn infer_trait_method_call_from_current_bounds(
    trait_name: &str,
    method_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Type> {
    let trait_signature = ctx.trait_signatures.get(trait_name)?;
    let inherited_methods = collect_trait_methods_with_inheritance(
        ctx.trait_signatures,
        trait_name,
        &mut HashMap::new(),
        &mut HashSet::new(),
    )?;
    let method_sig = inherited_methods.get(method_name)?;

    for bound in ctx
        .current_bounds
        .iter()
        .filter(|bound| bound.trait_name == trait_name)
    {
        if bound.trait_args.len() != trait_signature.type_params.len() {
            continue;
        }

        let mut method_vars = HashMap::new();
        let mut next_method_var = 0usize;
        for name in &trait_signature.type_params {
            method_vars.insert(normalize_type_param_name(name), next_method_var);
            next_method_var += 1;
        }

        let parsed_params = method_sig
            .params
            .iter()
            .map(|param| {
                parse_type_expr(
                    &param.ty,
                    ctx.alias_names,
                    &mut method_vars,
                    &mut next_method_var,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let parsed_return = parse_type_expr(
            &method_sig.return_type,
            ctx.alias_names,
            &mut method_vars,
            &mut next_method_var,
        )?;

        let mut trait_subst = HashMap::new();
        for (param_name, arg_type) in trait_signature.type_params.iter().zip(&bound.trait_args) {
            let var_id = *method_vars.get(&normalize_type_param_name(param_name))?;
            trait_subst.insert(var_id, arg_type.clone());
        }

        let bounds =
            parse_generic_bounds(&method_sig.generic_bounds, &method_vars, ctx.alias_names)
                .into_iter()
                .map(|method_bound| FunctionBound {
                    trait_name: method_bound.trait_name,
                    trait_args: method_bound
                        .trait_args
                        .into_iter()
                        .map(|arg| substitute_type_vars(&arg, &trait_subst))
                        .collect(),
                })
                .collect();
        let scheme = FunctionScheme {
            params: parsed_params
                .into_iter()
                .map(|param| substitute_type_vars(&param, &trait_subst))
                .collect(),
            ret: substitute_type_vars(&parsed_return, &trait_subst),
            generic_params: Vec::new(),
            bounds,
        };

        let mut trial_subst = subst.clone();
        if let Ok(return_type) =
            infer_function_with_bounds(&scheme, Some(method_name), arg_types, ctx, &mut trial_subst)
        {
            *subst = trial_subst;
            return Some(return_type);
        }
    }

    None
}

/// Checks whether a trait bound has a visible implementation.
///
/// Inputs:
/// - `trait_name`: required trait name.
/// - `bound_args`: canonicalized trait arguments.
/// - `ctx`: expression context with impl candidates and active bounds.
///
/// Output:
/// - `true` when an impl or active bound satisfies the requirement.
///
/// Transformation:
/// - Uses a cache for top-level lookups, compares impl arguments with
///   renaming-tolerant unification, and falls back to current bounds.
pub(crate) fn trait_has_bound_implementation(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    let cache_key = TraitBoundLookupKey {
        trait_name: trait_name.to_string(),
        bound_args: bound_args.to_vec(),
    };
    if ctx.current_bounds.is_empty() {
        let cache = ctx.trait_lookup_cache.borrow();
        if let Some(cached) = cache.bound_checks.get(&cache_key) {
            return *cached;
        }
    }

    let Some(candidates) = ctx.trait_bound_impl_type_args.get(trait_name) else {
        let found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
        if ctx.current_bounds.is_empty() {
            ctx.trait_lookup_cache
                .borrow_mut()
                .bound_checks
                .insert(cache_key, found);
        }
        return found;
    };

    let mut found = false;
    for impl_args in candidates {
        if impl_args.len() != bound_args.len() {
            continue;
        }

        let expanded_impl_args = impl_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();

        if types_unify_with_renaming(bound_args, &expanded_impl_args).is_ok() {
            found = true;
            break;
        }
    }

    if !found {
        found = current_bounds_satisfy_trait_bound(trait_name, bound_args, ctx);
    }

    if ctx.current_bounds.is_empty() {
        ctx.trait_lookup_cache
            .borrow_mut()
            .bound_checks
            .insert(cache_key, found);
    }
    found
}

/// Checks whether active generic bounds satisfy a requested trait bound.
///
/// Inputs:
/// - `trait_name`: trait being required, such as `Eq`.
/// - `bound_args`: canonicalized required trait arguments.
/// - `ctx`: expression inference context carrying the current callable bounds.
///
/// Output:
/// - `true` when one active callable bound has the same trait name and
///   unifies with `bound_args`; otherwise `false`.
///
/// Transformation:
/// - Expands local aliases in the active bound arguments and performs a
///   renaming-tolerant unification check without mutating inference
///   substitution state.
fn current_bounds_satisfy_trait_bound(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    ctx.current_bounds.iter().any(|bound| {
        if bound.trait_name != trait_name || bound.trait_args.len() != bound_args.len() {
            return false;
        }

        let active_args = bound
            .trait_args
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();
        types_unify_with_renaming(bound_args, &active_args).is_ok()
    })
}

/// Collects trait implementation argument shapes from resolved methods.
///
/// Inputs:
/// - `trait_method_calls`: resolved trait method dispatch table.
///
/// Output:
/// - Map from trait name to unique implemented type-argument vectors.
///
/// Transformation:
/// - Deduplicates implementation type arguments across methods so bound checks
///   can operate at trait level rather than method level.
pub(crate) fn collect_trait_bound_impl_type_args(
    trait_method_calls: &HashMap<(String, String), Vec<ResolvedTraitMethod>>,
) -> HashMap<String, Vec<Vec<Type>>> {
    let mut impl_type_args = HashMap::new();
    for ((trait_name, _), methods) in trait_method_calls {
        let candidates: &mut Vec<Vec<Type>> = impl_type_args.entry(trait_name.clone()).or_default();
        for method in methods {
            if candidates
                .iter()
                .any(|existing| existing == &method.impl_type_args)
            {
                continue;
            }
            candidates.push(method.impl_type_args.clone());
        }
    }
    impl_type_args
}

/// Unifies type lists while allowing type-variable renaming.
///
/// Inputs:
/// - `expected`: expected type arguments.
/// - `actual`: candidate type arguments.
///
/// Output:
/// - `Ok(())` when the lists unify after renaming candidate variables.
///
/// Transformation:
/// - Remaps candidate type-variable IDs into a fresh range before ordinary
///   unification.
fn types_unify_with_renaming(expected: &[Type], actual: &[Type]) -> Result<(), String> {
    let mut next_var = max_type_var_id(expected);
    let mut remap = HashMap::new();
    let normalized_actual = actual
        .iter()
        .map(|arg| remap_type_var_id(arg, &mut next_var, &mut remap))
        .collect::<Vec<_>>();

    let mut local_subst = HashMap::new();
    for (expected_arg, actual_arg) in expected.iter().zip(normalized_actual.iter()) {
        unify(expected_arg, actual_arg, &mut local_subst)?;
    }
    Ok(())
}

/// Finds the next free type-variable id after a list of types.
///
/// Inputs:
/// - `types`: type list to scan.
///
/// Output:
/// - One greater than the maximum contained type-variable id, or `0`.
///
/// Transformation:
/// - Traverses all nested type variables and computes the fresh-id lower bound.
fn max_type_var_id(types: &[Type]) -> TypeVarId {
    types
        .iter()
        .filter_map(max_type_var)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

/// Remaps type-variable IDs inside one type.
///
/// Inputs:
/// - `ty`: type to remap.
/// - `next_var`: next fresh variable id.
/// - `remap`: accumulated old-to-new id table.
///
/// Output:
/// - Type with remapped variable IDs.
///
/// Transformation:
/// - Rewrites type variables through `remap_type`, allocating fresh IDs for
///   previously unseen variables.
pub(crate) fn remap_type_var_id(
    ty: &Type,
    next_var: &mut TypeVarId,
    remap: &mut HashMap<TypeVarId, TypeVarId>,
) -> Type {
    remap_type(ty, &mut |id| {
        if let Some(remapped) = remap.get(id) {
            *remapped
        } else {
            let remapped = *next_var;
            remap.insert(*id, remapped);
            *next_var += 1;
            remapped
        }
    })
}

/// Refines local types using a simple syntax guard.
///
/// Inputs:
/// - `guard`: syntax-output guard expression.
/// - `locals`: mutable local type environment.
/// - `aliases` and `subst`: visible aliases and current substitutions.
///
/// Output:
/// - No direct return value; `locals` may be narrowed.
///
/// Transformation:
/// - Recognizes supported type-test guard calls and narrows the target local
///   when the narrowed type unifies with the existing type.
pub(crate) fn refine_by_syntax_guard(
    guard: &SyntaxExprOutput,
    locals: &mut HashMap<String, Type>,
    aliases: &HashMap<String, TypeAlias>,
    subst: &mut HashMap<TypeVarId, Type>,
) {
    if guard.kind != SyntaxExprKind::Call || guard.remote.is_some() || guard.children.len() != 2 {
        return;
    }

    let Some(callee_name) = syntax_callee_name(guard) else {
        return;
    };
    let Some(guard_target) = guard.children.get(1).and_then(|arg| match arg.kind {
        SyntaxExprKind::Var => arg.text.as_deref(),
        _ => None,
    }) else {
        return;
    };
    let Some(narrowed) = guard_narrow_type(callee_name) else {
        return;
    };

    if let Some(existing) = locals.get(guard_target) {
        if unify(existing, &narrowed, subst).is_ok() {
            let narrowed = expand_type_aliases(&narrowed, aliases);
            if let Some(value) = locals.get_mut(guard_target) {
                *value = narrowed;
            }
        }
    }
}

/// Canonicalizes trait lookup types for cache keys.
///
/// Inputs:
/// - `types`: trait lookup argument types.
///
/// Output:
/// - Type list with deterministic type-variable IDs.
///
/// Transformation:
/// - Remaps every type variable through a fresh dense ID sequence so equivalent
///   generic lookups share cache keys even when they came from different
///   instantiation sites.
pub(crate) fn canonicalize_trait_lookup_types(types: &[Type]) -> Vec<Type> {
    let mut next_var = 0usize;
    let mut remap = HashMap::new();
    types
        .iter()
        .map(|ty| remap_type_var_id(ty, &mut next_var, &mut remap))
        .collect()
}
