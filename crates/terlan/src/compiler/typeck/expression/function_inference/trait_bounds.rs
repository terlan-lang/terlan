use super::*;

mod structural_implication;
mod trait_bound_refinement;
pub(crate) use structural_implication::structural_implication_field_type;
use structural_implication::{check_structural_implication_bound, structural_call_subst};
pub(crate) use trait_bound_refinement::refine_call_local_trait_bounds;

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
    imported_bound_impls: HashMap<String, Vec<Vec<Type>>>,
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
/// - Trait name, method name, explicit implementation target, and call argument types.
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
    pub(crate) explicit_target: Option<Type>,
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
    check_function_bounds_with_structural_subst(scheme, function_name, ctx, subst, subst)
}

/// Checks function bounds at a call site while retaining concrete structural
/// evidence that ordinary dynamic unification intentionally does not bind.
pub(crate) fn check_function_call_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    for bound in &scheme.bounds {
        if !is_js_callable_bound(&bound.trait_name) {
            continue;
        }
        let [subject] = bound.trait_args.as_slice() else {
            continue;
        };
        for (parameter, argument) in scheme.params.iter().zip(args) {
            if parameter == subject && !has_js_callable_evidence(argument, ctx, subst) {
                return Err(format!(
                    "at `{}` call site: expected checked JavaScript callable, found {}",
                    function_name.unwrap_or("expression"),
                    pretty_type(&apply_subst(argument, subst)),
                ));
            }
        }
    }
    let structural_subst = structural_call_subst(scheme, args, subst);
    check_function_bounds_with_structural_subst(
        scheme,
        function_name,
        ctx,
        subst,
        &structural_subst,
    )
}

fn has_js_callable_evidence(
    subject: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let subject = apply_subst(subject, subst);
    let subject = qualify_imported_named_heads(&subject, ctx);
    match expand_type_aliases(&subject, ctx.aliases) {
        Type::Function { .. } => true,
        Type::Named { module: Some(module), name, args }
            if module == "std.js.Callable" && name == "VariadicCallable" && args.len() == 1 =>
        {
            true
        }
        Type::Union(members) => !members.is_empty()
            && members.iter().all(|member| has_js_callable_evidence(member, ctx, subst)),
        subject @ Type::Var(_) => ctx.current_bounds.iter().any(|bound| {
            is_js_callable_bound(&bound.trait_name)
                && matches!(bound.trait_args.as_slice(), [evidence] if apply_subst(evidence, subst) == subject)
        }),
        _ => false,
    }
}

fn check_function_bounds_with_structural_subst(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
    structural_subst: &HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    if scheme.bounds.is_empty() {
        return Ok(());
    }

    for bound in &scheme.bounds {
        let bound_subst = if bound.trait_name == STRUCTURAL_IMPLICATION_BOUND {
            structural_subst
        } else {
            subst
        };
        let resolved_args = bound
            .trait_args
            .iter()
            .map(|arg| {
                let arg = apply_subst(arg, bound_subst);
                let arg = qualify_imported_named_heads(&arg, ctx);
                expand_type_aliases(&arg, ctx.aliases)
            })
            .collect::<Vec<_>>();
        let resolved_args = canonicalize_trait_lookup_types(&resolved_args);
        let visible_trait_name = source_visible_trait_name(&bound.trait_name, ctx);
        let trait_description = if resolved_args.is_empty() {
            visible_trait_name.to_string()
        } else {
            format!(
                "{}[{}]",
                visible_trait_name,
                resolved_args
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        let context = function_name.unwrap_or("expression");

        if bound.trait_name == STRUCTURAL_IMPLICATION_BOUND {
            check_structural_implication_bound(&resolved_args, context, ctx, structural_subst)?;
            continue;
        }

        if is_js_callable_bound(&bound.trait_name) {
            if !matches!(resolved_args.as_slice(), [subject] if has_js_callable_evidence(subject, ctx, bound_subst))
            {
                return Err(format!("at `{context}` call site: expected checked JavaScript callable `{trait_description}`"));
            }
            continue;
        }

        if trait_bound_is_explicitly_denied(&bound.trait_name, &resolved_args, ctx) {
            return Err(format!(
                "at `{}` call site: trait bound `{}` is explicitly denied",
                context, trait_description
            ));
        }

        if !trait_has_bound_implementation(&bound.trait_name, &resolved_args, ctx) {
            return Err(format!(
                "at `{}` call site: expected trait bound `{}`",
                context, trait_description
            ));
        }
    }

    Ok(())
}

/// Keeps diagnostics aligned with the trait spelling visible at the call site.
fn source_visible_trait_name<'a>(trait_name: &'a str, ctx: &ExprInferContext<'_>) -> &'a str {
    trait_name
        .rsplit('.')
        .next()
        .filter(|local_name| {
            *local_name != trait_name && ctx.trait_signatures.contains_key(*local_name)
        })
        .unwrap_or(trait_name)
}

/// Checks concrete trait arguments against visible explicit denials.
fn trait_bound_is_explicitly_denied(
    trait_name: &str,
    bound_args: &[Type],
    ctx: &ExprInferContext<'_>,
) -> bool {
    if bound_args
        .iter()
        .any(|arg| !trait_bound_type_is_concrete(arg))
    {
        return false;
    }
    let mut candidates = ctx
        .negative_trait_impl_type_args
        .get(trait_name)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if let Some(local_name) = trait_name.rsplit('.').next().filter(|local_name| {
        *local_name != trait_name && ctx.trait_signatures.contains_key(*local_name)
    }) {
        if let Some(local_candidates) = ctx.negative_trait_impl_type_args.get(local_name) {
            candidates.extend(local_candidates);
        }
    }
    candidates.into_iter().any(|candidate| {
        let expanded = candidate
            .iter()
            .map(|arg| expand_type_aliases(arg, ctx.aliases))
            .collect::<Vec<_>>();
        types_unify_with_renaming(bound_args, &expanded).is_ok()
    })
}

/// Returns whether a bound argument is resolved enough for denial matching.
fn trait_bound_type_is_concrete(ty: &Type) -> bool {
    match ty {
        Type::Var(_) | Type::Apply { .. } | Type::Placeholder | Type::Existential { .. } => false,
        Type::Named { args, .. } => args.iter().all(trait_bound_type_is_concrete),
        Type::List(inner) => trait_bound_type_is_concrete(inner),
        Type::Array { size, elem } => {
            matches!(size, crate::terlan_typeck::types::ArraySize::Known(_))
                && trait_bound_type_is_concrete(elem)
        }
        Type::Tuple(items) | Type::Union(items) => items.iter().all(trait_bound_type_is_concrete),
        Type::Map(fields) => fields
            .iter()
            .all(|field| trait_bound_type_is_concrete(&field.value)),
        Type::Function { params, ret } => params
            .iter()
            .chain(std::iter::once(ret.as_ref()))
            .all(trait_bound_type_is_concrete),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::LiteralBool(_) => true,
    }
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

        let bounds: Vec<FunctionBound> =
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
                .map(|param| {
                    qualify_imported_named_heads(&substitute_type_vars(&param, &trait_subst), ctx)
                })
                .collect(),
            ret: qualify_imported_named_heads(
                &substitute_type_vars(&parsed_return, &trait_subst),
                ctx,
            ),
            generic_params: Vec::new(),
            generic_vars: Vec::new(),
            bounds: bounds
                .into_iter()
                .map(|bound| FunctionBound {
                    trait_name: bound.trait_name,
                    trait_args: bound
                        .trait_args
                        .iter()
                        .map(|arg| qualify_imported_named_heads(arg, ctx))
                        .collect(),
                })
                .collect(),
        };

        let mut trial_subst = subst.clone();
        if let Ok(return_type) = infer_instantiated_function_with_bounds_from(
            &scheme,
            Some(method_name),
            arg_types,
            ctx,
            &mut trial_subst,
            None,
        ) {
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

    let candidates = trait_bound_implementation_candidates(trait_name, ctx);

    let mut found = false;
    for impl_args in &candidates {
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

/// Collects every visible implementation head for one trait bound.
///
/// Imported conformances participate in inference as well as final bound
/// validation. Otherwise output-only generic parameters cannot be recovered
/// from an implementation loaded through a package interface.
fn trait_bound_implementation_candidates(
    trait_name: &str,
    ctx: &ExprInferContext<'_>,
) -> Vec<Vec<Type>> {
    let mut candidates = ctx
        .trait_bound_impl_type_args
        .get(trait_name)
        .cloned()
        .unwrap_or_default();
    if let Some(local_name) = trait_name.rsplit('.').next().filter(|local_name| {
        *local_name != trait_name && ctx.trait_signatures.contains_key(*local_name)
    }) {
        if let Some(local_candidates) = ctx.trait_bound_impl_type_args.get(local_name) {
            for candidate in local_candidates {
                if !candidates.contains(candidate) {
                    candidates.push(candidate.clone());
                }
            }
        }
    }
    for imported in imported_interface_trait_bound_impls(trait_name, ctx) {
        if !candidates.contains(&imported) {
            candidates.push(imported);
        }
    }
    candidates
}

/// Loads provider conformance facts required only by imported function bounds.
///
/// Selected function imports retain declaration-site trait constraints without
/// making that trait a callable namespace in the consumer. The ordinary method
/// candidate map intentionally excludes those traits, so this lookup indexes
/// their public conformance facts lazily and caches the result per module.
fn imported_interface_trait_bound_impls(
    trait_name: &str,
    ctx: &ExprInferContext<'_>,
) -> Vec<Vec<Type>> {
    if let Some(cached) = ctx
        .trait_lookup_cache
        .borrow()
        .imported_bound_impls
        .get(trait_name)
        .cloned()
    {
        return cached;
    }

    let mut candidates = Vec::new();
    for interface in ctx.interface_map.values() {
        for conformance in &interface.trait_conformances {
            if !conformance.public || conformance.is_negative {
                continue;
            }
            let Some(implemented_trait) = parse_trait_instance_from_text(&conformance.trait_ref)
            else {
                continue;
            };
            let short_name = implemented_trait
                .name
                .rsplit('.')
                .next()
                .unwrap_or(&implemented_trait.name);
            let provider_local_trait = !implemented_trait.name.contains('.')
                && interface.traits.contains_key(&implemented_trait.name)
                && crate::terlan_hir::public_type_path(&interface.module, &implemented_trait.name)
                    == trait_name;
            let matches = if trait_name.contains('.') {
                implemented_trait.name == trait_name || provider_local_trait
            } else {
                implemented_trait.name == trait_name || short_name == trait_name
            };
            if !matches {
                continue;
            }
            let qualified_args =
                crate::compiler::typeck::trait_model::qualify_interface_trait_type_args(
                    &implemented_trait.type_args,
                    interface,
                    ctx.interface_map,
                );
            let mut vars = conformance
                .generic_params
                .iter()
                .enumerate()
                .map(|(index, param)| (normalize_type_param_name(param), index))
                .collect::<HashMap<_, _>>();
            let mut next_var = vars.len();
            let Some(parsed_args) = qualified_args
                .iter()
                .map(|arg| parse_type_expr(arg, ctx.alias_names, &mut vars, &mut next_var))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if !candidates.iter().any(|candidate| candidate == &parsed_args) {
                candidates.push(parsed_args);
            }
        }
    }

    ctx.trait_lookup_cache
        .borrow_mut()
        .imported_bound_impls
        .insert(trait_name.to_string(), candidates.clone());
    candidates
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
        let same_trait = bound.trait_name == trait_name
            || trait_name.rsplit('.').next().is_some_and(|local_name| {
                local_name == bound.trait_name && ctx.trait_signatures.contains_key(local_name)
            })
            || bound
                .trait_name
                .rsplit('.')
                .next()
                .is_some_and(|local_name| {
                    local_name == trait_name && ctx.trait_signatures.contains_key(local_name)
                });
        if !same_trait || bound.trait_args.len() != bound_args.len() {
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
    let mut required_variables = HashSet::new();
    for argument in expected {
        remap_type(argument, &mut |id| {
            required_variables.insert(*id);
            *id
        });
    }
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
    // Implementation variables may specialize to the requested types. The
    // caller's generic variables must stay universally quantified: a concrete
    // Show[Int] instance cannot prove Show[T] for an arbitrary T. Distinct
    // caller variables must also remain distinct after matching.
    let mut remaining_variables = HashSet::new();
    for variable in required_variables {
        match apply_subst(&Type::Var(variable), &local_subst) {
            Type::Var(id) if remaining_variables.insert(id) => {}
            _ => return Err("trait implementation restricts a generic argument".into()),
        }
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
