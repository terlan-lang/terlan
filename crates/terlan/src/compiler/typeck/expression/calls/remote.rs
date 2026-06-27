use super::*;

/// Infers an explicit remote call.
///
/// Inputs:
/// - `expr`: call expression with a remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `type_args`: explicit generic call arguments from `Module.fun[Type](...)`.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved remote call return type.
///
/// Transformation:
/// - Resolves imported modules, trait calls, target intrinsics, and interface
///   functions before falling back to dynamic typing with diagnostics.
pub(super) fn infer_syntax_remote_call(
    module_name: &str,
    function_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let (trait_remote_name, explicit_trait_type_arg) =
        split_explicit_trait_call_target(module_name, ctx);
    let resolved_module_name = ctx
        .module_aliases
        .get(&trait_remote_name)
        .map(String::as_str)
        .unwrap_or(trait_remote_name.as_str());

    if resolved_module_name == "Html" && function_name == "raw" {
        if arg_types.len() != 1 {
            errors.push(format!(
                "function arity mismatch: expected 1 args, found {}",
                arg_types.len()
            ));
            return Type::Dynamic;
        }
        if let Err(message) = unify(&Type::Binary, &arg_types[0], subst) {
            errors.push(message);
        }
        return Type::Named {
            module: None,
            name: "Html".to_string(),
            args: vec![Type::Dynamic],
        };
    }

    let trait_key = (resolved_module_name.to_string(), function_name.to_string());
    if let Some(impls) = ctx.trait_method_calls.get(&trait_key) {
        let lookup_arg_types = arg_types
            .iter()
            .map(|arg| apply_subst(arg, subst))
            .collect::<Vec<_>>();
        let dispatch_arg_types = lookup_arg_types
            .iter()
            .map(|arg| qualify_imported_named_heads(arg, ctx))
            .collect::<Vec<_>>();
        let cached_lookup_arg_types =
            canonicalize_trait_lookup_types(dispatch_arg_types.as_slice());
        let lookup_key = TraitMethodLookupKey {
            trait_name: resolved_module_name.to_string(),
            method_name: function_name.to_string(),
            arg_types: cached_lookup_arg_types,
        };
        let lookup_result = {
            let cache = ctx.trait_lookup_cache.borrow();
            if let Some(cached) = cache.method_calls.get(&lookup_key).copied() {
                Some(cached)
            } else {
                drop(cache);
                let mut matching = None::<usize>;
                let mut matches = 0usize;
                for (index, impl_candidate) in impls.iter().enumerate() {
                    if !explicit_trait_type_arg.as_ref().is_none_or(|expected| {
                        trait_candidate_matches_explicit_type_arg(
                            impl_candidate,
                            expected,
                            ctx,
                            subst,
                        )
                    }) {
                        continue;
                    }
                    if !trait_method_candidate_matches_call(
                        impl_candidate,
                        &dispatch_arg_types,
                        ctx,
                        subst,
                    ) {
                        continue;
                    }
                    let mut trial_subst = subst.clone();
                    if infer_function_with_bounds(
                        &impl_candidate.scheme,
                        None,
                        &dispatch_arg_types,
                        ctx,
                        &mut trial_subst,
                    )
                    .is_ok()
                    {
                        matches += 1;
                        if matching.is_none() {
                            matching = Some(index);
                        } else {
                            break;
                        }
                    }
                }

                let resolved = match matching {
                    None => TraitMethodLookupResult::NoMatch,
                    Some(index) if matches == 1 => TraitMethodLookupResult::Single(index),
                    Some(_) => TraitMethodLookupResult::Ambiguous,
                };
                ctx.trait_lookup_cache
                    .borrow_mut()
                    .method_calls
                    .insert(lookup_key, resolved);
                Some(resolved)
            }
        };

        let provided_args = arg_types
            .iter()
            .map(pretty_type)
            .collect::<Vec<_>>()
            .join(", ");
        match lookup_result {
            Some(TraitMethodLookupResult::Single(index)) => {
                let mut inferred_subst = subst.clone();
                let mut success = None::<(Type, HashMap<TypeVarId, Type>)>;
                if let Some(impl_candidate) = impls.get(index) {
                    if let Ok(ty) = infer_function_with_bounds(
                        &impl_candidate.scheme,
                        None,
                        &dispatch_arg_types,
                        ctx,
                        &mut inferred_subst,
                    ) {
                        success = Some((ty, inferred_subst));
                    }
                }
                if let Some((ty, inferred_subst)) = success {
                    *subst = inferred_subst;
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
            Some(TraitMethodLookupResult::Ambiguous) => {
                errors.push(format!(
                    "at `{}.{}` call site: ambiguous trait method {}.{}",
                    resolved_module_name, function_name, resolved_module_name, function_name
                ));
                return Type::Dynamic;
            }
            _ => {
                if let Some(ty) = infer_trait_method_call_from_current_bounds(
                    resolved_module_name,
                    function_name,
                    &lookup_arg_types,
                    ctx,
                    subst,
                ) {
                    return ty;
                }
                errors.push(format!(
                    "at `{}.{}` call site: no impl for trait method {}.{} with provided arguments [{}]",
                    resolved_module_name, function_name, resolved_module_name, function_name, provided_args
                ));
                return Type::Dynamic;
            }
        }
    }

    if let Some(interface) = ctx.interface_map.get(resolved_module_name) {
        let candidate_signatures =
            interface_function_signatures(interface, function_name, arg_types.len());
        let effective_arg_types = if !candidate_signatures.is_empty() {
            let mut default_errors = Vec::new();
            match complete_defaulted_imported_call_args_for_any_signature(
                &format!("{}.{}", resolved_module_name, function_name),
                arg_types,
                arg_names,
                &candidate_signatures,
                interface,
                ctx,
                &mut default_errors,
            ) {
                Some(completed) => completed,
                None => {
                    errors.extend(default_errors);
                    return Type::Dynamic;
                }
            }
        } else {
            arg_types.to_vec()
        };
        match infer_interface_function_overload_with_explicit_type_args(
            interface,
            function_name,
            &format!("{}.{}", resolved_module_name, function_name),
            &effective_arg_types,
            type_args,
            ctx,
            subst,
        ) {
            Ok(Some(ty)) => return ty,
            Ok(None) => {}
            Err(message) => {
                errors.push(message);
                return Type::Dynamic;
            }
        }
        if let Some(schemes) = parse_interface_constructor_schemes(
            interface.constructors.get(function_name).map(Vec::as_slice),
            interface,
        ) {
            if let Some(constructed) = infer_constructor_schemes(
                function_name,
                &schemes,
                arg_types,
                arg_names,
                subst,
                errors,
            ) {
                let interface_aliases = interface_type_aliases(interface);
                return expand_type_aliases(&constructed, &interface_aliases);
            }
        }
        let interface_aliases = interface_type_aliases(interface);
        let qualified_alias_name = format!("{}.{}", resolved_module_name, function_name);
        let mut qualified_aliases = interface_aliases.clone();
        if let Some(alias) = interface_aliases.get(function_name) {
            qualified_aliases.insert(qualified_alias_name.clone(), alias.clone());
        }
        if let Some(schemes) =
            alias_constructor_call_schemes(&qualified_alias_name, &qualified_aliases)
        {
            if let Some(constructed) = infer_constructor_schemes(
                function_name,
                &schemes,
                arg_types,
                arg_names,
                subst,
                errors,
            ) {
                return expand_type_aliases(&constructed, &qualified_aliases);
            }
        }
        if function_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
            && interface.opaque_types.contains(function_name)
        {
            errors.push(format!(
                "cannot construct opaque type {}.{} outside defining module",
                resolved_module_name, function_name
            ));
            return Type::Dynamic;
        }
    }

    if is_constructor_name(function_name) {
        errors.push(format!(
            "unknown constructor {}.{} / {}",
            resolved_module_name,
            function_name,
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if resolved_module_name == "Group" && function_name == "broadcast" && arg_types.len() == 2 {
        if let Type::Named {
            name,
            args: group_args,
            ..
        } = &arg_types[0]
        {
            if name == "Group" && group_args.len() == 1 {
                if let Err(message) = unify(&group_args[0], &arg_types[1], subst) {
                    let expected = alias_name_for_type(&group_args[0], ctx.aliases)
                        .unwrap_or_else(|| pretty_type(&group_args[0]));
                    errors.push(format!(
                        "expected {} found {}",
                        expected,
                        pretty_type(&arg_types[1])
                    ));
                    let _ = message;
                }
            }
        }
        return Type::LiteralAtom("ok".to_string());
    }

    if (resolved_module_name == "Route" || resolved_module_name.ends_with(".Route"))
        && function_name == "to_path"
        && arg_types.len() == 1
    {
        return Type::Binary;
    }

    errors.push(unresolved_remote_call_message(
        resolved_module_name,
        function_name,
        arg_types.len(),
        ctx,
    ));
    Type::Dynamic
}

/// Builds a diagnostic for an unresolved qualified call.
///
/// Inputs:
/// - `module_name`: resolved remote module head from the call.
/// - `function_name`: function segment from the qualified call.
/// - `arity`: number of supplied call arguments.
/// - `ctx`: expression inference context containing loaded interfaces.
///
/// Output:
/// - Human-facing diagnostic explaining whether the module or function is
///   missing.
///
/// Transformation:
/// - Distinguishes an unknown module from a known module with no matching
///   public function so compiler diagnostics catch issues that would otherwise
///   lower into backend-specific runtime failures.
fn unresolved_remote_call_message(
    module_name: &str,
    function_name: &str,
    arity: usize,
    ctx: &ExprInferContext,
) -> String {
    if ctx.interface_map.contains_key(module_name) {
        format!(
            "module `{}` has no public function `{}/{}`",
            module_name, function_name, arity
        )
    } else {
        format!(
            "cannot resolve module `{}` for call `{}.{}/{}`",
            module_name, module_name, function_name, arity
        )
    }
}

/// Checks whether a trait candidate can own the current call.
///
/// Inputs:
/// - `candidate`: resolved trait method candidate with concrete impl type args.
/// - `arg_types`: inferred source-visible call argument types.
/// - `ctx`: expression inference context containing alias expansion rules.
/// - `subst`: current type-variable substitution table.
///
/// Output:
/// - `true` when the candidate has no owner type information, when its
///   specialized first parameter unifies with the call's first argument type,
///   or when its first impl type argument unifies with that argument type.
/// - `false` when a different concrete conformance owns the method.
///
/// Transformation:
/// - Uses cloned substitution tables and transparent alias expansion to filter
///   trait method candidates before ambiguity counting. Specialized parameter
///   matching handles higher-kinded impls such as `Functor[Option]`, while the
///   owner fallback keeps imported multi-conformance traits such as
///   `std.core.String.Show` from treating `Show[Int]`, `Show[Bool]`, and
///   `Show[String]` as simultaneous matches for one receiver/value argument.
pub(crate) fn trait_method_candidate_matches_call(
    candidate: &ResolvedTraitMethod,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    if let (Some(first_param_type), Some(first_arg_type)) =
        (candidate.scheme.params.first(), arg_types.first())
    {
        let mut trial_subst = subst.clone();
        if unify(first_param_type, first_arg_type, &mut trial_subst).is_ok() {
            return true;
        }

        let param_expanded = expand_type_aliases(first_param_type, ctx.aliases);
        let arg_expanded = expand_type_aliases(first_arg_type, ctx.aliases);
        let mut expanded_subst = subst.clone();
        if unify(&param_expanded, &arg_expanded, &mut expanded_subst).is_ok() {
            return true;
        }

        let param_qualified = qualify_imported_named_heads(first_param_type, ctx);
        let arg_qualified = qualify_imported_named_heads(first_arg_type, ctx);
        let mut qualified_subst = subst.clone();
        if unify(&param_qualified, &arg_qualified, &mut qualified_subst).is_ok() {
            return true;
        }
    }

    let Some(owner_type) = candidate.impl_type_args.first() else {
        return true;
    };
    let Some(first_arg_type) = arg_types.first() else {
        return true;
    };

    let mut trial_subst = subst.clone();
    if unify(owner_type, first_arg_type, &mut trial_subst).is_ok() {
        return true;
    }

    let owner_expanded = expand_type_aliases(owner_type, ctx.aliases);
    let arg_expanded = expand_type_aliases(first_arg_type, ctx.aliases);
    if unify(&owner_expanded, &arg_expanded, &mut trial_subst).is_ok() {
        return true;
    }

    let owner_qualified = qualify_imported_named_heads(owner_type, ctx);
    let arg_qualified = qualify_imported_named_heads(first_arg_type, ctx);
    let mut qualified_subst = subst.clone();
    unify(&owner_qualified, &arg_qualified, &mut qualified_subst).is_ok()
}

/// Splits a possible explicit trait call target.
///
/// Inputs:
/// - `remote`: source remote qualifier such as `Functor[Option]` or `Module`.
/// - `ctx`: active expression inference context with visible type aliases.
///
/// Output:
/// - Remote head name and optional parsed explicit trait implementation type.
///
/// Transformation:
/// - Recognizes the closed `Trait[Type]` form used for explicit trait dispatch,
///   parses the first type argument through the normal type parser, and leaves
///   all ordinary remote qualifiers unchanged.
fn split_explicit_trait_call_target(
    remote: &str,
    ctx: &ExprInferContext<'_>,
) -> (String, Option<Type>) {
    let Some((head, raw_args)) = remote.split_once('[') else {
        return (remote.to_string(), None);
    };
    let Some(raw_arg) = raw_args.strip_suffix(']') else {
        return (remote.to_string(), None);
    };
    let head = head.trim();
    let raw_arg = raw_arg.trim();
    if head.is_empty() || raw_arg.is_empty() || raw_arg.contains(',') {
        return (remote.to_string(), None);
    }

    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let mut alias_names = ctx.aliases.keys().cloned().collect::<HashSet<_>>();
    alias_names.extend(ctx.imported_type_names.keys().cloned());
    let parsed = parse_type_expr(raw_arg, &alias_names, &mut vars, &mut next_var);
    (head.to_string(), parsed)
}

/// Checks one trait candidate against an explicit implementation type target.
///
/// Inputs:
/// - `candidate`: resolved trait method candidate.
/// - `expected`: explicit type argument from `Trait[Type].method(...)`.
/// - `ctx` and `subst`: active type aliases and substitutions.
///
/// Output:
/// - `true` when the candidate's first implementation type matches `expected`.
///
/// Transformation:
/// - Applies current substitutions, tries direct unification, and then retries
///   after transparent alias expansion so imported aliases such as `Option`
///   match their provider-qualified implementation type.
fn trait_candidate_matches_explicit_type_arg(
    candidate: &ResolvedTraitMethod,
    expected: &Type,
    ctx: &ExprInferContext<'_>,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let Some(owner_type) = candidate.impl_type_args.first() else {
        return false;
    };
    let owner = apply_subst(owner_type, subst);
    let expected = apply_subst(expected, subst);

    let mut direct_subst = subst.clone();
    if unify(&owner, &expected, &mut direct_subst).is_ok() {
        return true;
    }
    if named_type_heads_match(&owner, &expected) {
        return true;
    }

    let owner_expanded = expand_type_aliases(&owner, ctx.aliases);
    let expected_expanded = expand_type_aliases(&expected, ctx.aliases);
    let mut expanded_subst = subst.clone();
    if unify(&owner_expanded, &expected_expanded, &mut expanded_subst).is_ok() {
        return true;
    }

    let owner_qualified = qualify_imported_named_heads(&owner, ctx);
    let expected_qualified = qualify_imported_named_heads(&expected, ctx);
    let mut qualified_subst = subst.clone();
    unify(&owner_qualified, &expected_qualified, &mut qualified_subst).is_ok()
}

/// Checks whether two named type heads represent the same source type name.
///
/// Inputs:
/// - `left` and `right`: candidate implementation and explicit target types.
///
/// Output:
/// - `true` when both are named types with the same final type constructor
///   segment.
///
/// Transformation:
/// - Ignores module qualification for explicit trait target matching so a
///   consumer import like `Option` can target a provider conformance recorded
///   as `std.core.Option.Option`.
fn named_type_heads_match(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (
            Type::Named {
                name: left_name, ..
            },
            Type::Named {
                name: right_name, ..
            },
        ) => left_name == right_name,
        _ => false,
    }
}

/// Qualifies imported local type names inside one inferred type tree.
///
/// Inputs:
/// - `ty`: source-visible type inferred at a call site or stored on a trait
///   candidate.
/// - `ctx`: expression context containing selected/default imported type names.
///
/// Output:
/// - Type tree with imported unqualified named heads rewritten to their
///   provider-qualified nominal names.
///
/// Transformation:
/// - Recurses through generic arguments, tuple/list/map/function shapes, and
///   higher-kinded applications so imported opaque types such as
///   `List[Int]` compare with provider conformances recorded as
///   `std.collections.List.List[Int]`.
pub(super) fn qualify_imported_named_heads(ty: &Type, ctx: &ExprInferContext<'_>) -> Type {
    match ty {
        Type::Named { module, name, args } => {
            let args = args
                .iter()
                .map(|arg| qualify_imported_named_heads(arg, ctx))
                .collect::<Vec<_>>();
            if module.is_none() {
                if let Some(imported) = ctx.imported_type_names.get(name) {
                    return Type::Named {
                        module: Some(imported.module.clone()),
                        name: imported.name.clone(),
                        args,
                    };
                }
            }
            Type::Named {
                module: module.clone(),
                name: name.clone(),
                args,
            }
        }
        Type::Apply { constructor, args } => Type::Apply {
            constructor: *constructor,
            args: args
                .iter()
                .map(|arg| qualify_imported_named_heads(arg, ctx))
                .collect(),
        },
        Type::List(inner) => {
            let inner = qualify_imported_named_heads(inner, ctx);
            if let Some(imported) = ctx.imported_type_names.get("List") {
                Type::Named {
                    module: Some(imported.module.clone()),
                    name: imported.name.clone(),
                    args: vec![inner],
                }
            } else {
                Type::List(Box::new(inner))
            }
        }
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| qualify_imported_named_heads(item, ctx))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| qualify_imported_named_heads(item, ctx))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| super::super::MapFieldType {
                    key: field.key.clone(),
                    value: qualify_imported_named_heads(&field.value, ctx),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| qualify_imported_named_heads(param, ctx))
                .collect(),
            ret: Box::new(qualify_imported_named_heads(ret, ctx)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(qualify_imported_named_heads(elem, ctx)),
        },
        other => other.clone(),
    }
}
