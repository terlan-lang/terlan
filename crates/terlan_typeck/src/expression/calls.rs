use super::*;

/// Infers a named call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Infers argument types first, then routes the call through constructor,
///   local, remote, receiver, trait, intrinsic, and import dispatch.
pub(super) fn infer_syntax_call_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Infers a dedicated function-value invocation expression.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression whose first child is the
///   callable expression and remaining children are call arguments.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The invoked function's return type when the callee has a function type.
/// - `Dynamic` when the callee is malformed, non-callable, or has invalid
///   argument types.
///
/// Transformation:
/// - Infers all non-callee arguments, then delegates to the shared
///   function-value invocation checker so pipe-forward can prepend a synthetic
///   first argument without rebuilding syntax nodes.
pub(super) fn infer_syntax_function_value_call(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arg_types = expr
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    infer_syntax_function_value_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Checks a function-value invocation with already inferred argument types.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression with a callable child.
/// - `arg_types`: argument types to check against the callee's function type.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - The callee return type with substitutions applied.
/// - `Dynamic` when the callee is not a function or arguments do not match.
///
/// Transformation:
/// - Infers the callee expression, requires a `Type::Function`, unifies each
///   parameter with the provided argument type, and returns the substituted
///   result type.
fn infer_syntax_function_value_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(callee) = expr.children.first() else {
        errors.push("function-value invocation is missing a callee".to_string());
        return Type::Dynamic;
    };

    let callee_type = apply_subst(
        &infer_syntax_expr(callee, locals, ctx, subst, errors),
        subst,
    );
    match callee_type {
        Type::Function { params, ret } => {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            apply_subst(ret.as_ref(), subst)
        }
        Type::Dynamic => Type::Dynamic,
        other => {
            errors.push(format!(
                "function-value invocation requires function value, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

/// Infers a resolved macro call.
///
/// Inputs:
/// - `macro_name`: source macro identifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Macro return type when a visible macro signature matches.
///
/// Transformation:
/// - Looks up macro-call signatures and checks arguments through ordinary
///   function inference, unwrapping macro-specific return wrappers.
pub(super) fn infer_syntax_macro_call(
    name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let candidates: Vec<_> = ctx
        .signatures
        .iter()
        .filter_map(|((candidate_name, arity), schemes)| {
            if candidate_name == name {
                Some((*arity, schemes))
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    for (arity, scheme) in candidates.iter() {
        if *arity == arg_types.len() {
            match infer_function_scheme_overload(scheme, name, arg_types, ctx, subst) {
                Ok(ty) => return Some(unwrap_macro_return_type(ty)),
                Err(message) => {
                    errors.push(message);
                    return Some(Type::Dynamic);
                }
            }
        }
    }

    let arities = candidates
        .iter()
        .map(|(arity, _)| arity.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "wrong arity for macro `{}`: expected one of [{}] args, found {}",
        name,
        arities,
        arg_types.len()
    ));
    Some(Type::Dynamic)
}

/// Infers a call expression after argument types are known.
///
/// Inputs:
/// - `expr`: call expression.
/// - `arg_types`: previously inferred argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved call return type.
///
/// Transformation:
/// - Applies call resolution without re-inferring arguments, allowing pipe and
///   function-value callers to share dispatch.
fn infer_syntax_call_with_arg_types(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if expr.remote.is_none() {
        if let Some(ty) =
            infer_syntax_primitive_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
        if let Some(ty) =
            infer_syntax_receiver_method_call(expr, arg_types, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let Some(function_name) = syntax_callee_name(expr) else {
        return Type::Dynamic;
    };

    if expr.remote.is_none() && syntax_callee_is_var(expr) {
        if let Some(constructed) =
            infer_constructor_call(function_name, &arg_types, ctx, subst, errors)
        {
            return constructed;
        }

        if let Some(imported) = ctx.constructor_aliases.get(function_name) {
            if let Some(interface) = ctx.interface_map.get(&imported.module) {
                if interface.opaque_types.contains(&imported.name) {
                    errors.push(format!(
                        "cannot construct opaque type {}.{} outside defining module",
                        imported.module, imported.name
                    ));
                    return Type::Dynamic;
                }
                if let Some(schemes) = parse_interface_constructor_schemes(
                    interface
                        .constructors
                        .get(&imported.name)
                        .map(Vec::as_slice),
                    interface,
                ) {
                    if let Some(constructed) = infer_constructor_schemes(
                        function_name,
                        &schemes,
                        &arg_types,
                        subst,
                        errors,
                    ) {
                        let interface_aliases = interface_type_aliases(interface);
                        return expand_type_aliases(&constructed, &interface_aliases);
                    }
                }
            }
        }

        if let Some(constructed) =
            infer_opaque_constructor(function_name, &arg_types, ctx.aliases, errors)
        {
            return constructed;
        }

        if let Some(Type::Function { params, ret }) =
            locals.get(function_name).map(|ty| apply_subst(ty, subst))
        {
            if params.len() != arg_types.len() {
                errors.push(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params.len(),
                    arg_types.len()
                ));
                return Type::Dynamic;
            }

            for (expected, actual) in params.iter().zip(arg_types.iter()) {
                if let Err(message) = unify(expected, actual, subst) {
                    errors.push(message);
                }
            }

            return apply_subst(ret.as_ref(), subst);
        }

        if is_constructor_name(function_name) {
            errors.push(format!(
                "unknown constructor {} / {}",
                function_name,
                arg_types.len()
            ));
            return Type::Dynamic;
        }
    }

    if let Some(module_name) = expr.remote.as_deref() {
        return infer_syntax_remote_call(module_name, function_name, arg_types, ctx, subst, errors);
    }

    infer_syntax_local_call(function_name, arg_types, ctx, subst, errors)
}

/// Returns whether a local function can also accept a pipe-inserted call.
///
/// Inputs:
/// - `function_name`: unqualified pipe target name.
/// - `arg_types`: pipe-inserted argument types, including the receiver/input as
///   the first argument.
/// - `ctx` and `subst`: active inference context and current substitutions.
///
/// Output:
/// - `true` when a local function signature or resolved local function symbol
///   can accept the same pipe-inserted call.
/// - `false` when no local function candidate matches.
///
/// Transformation:
/// - Tries explicit source function schemes with cloned substitutions so
///   ambiguity detection does not mutate the real inference state or emit
///   diagnostics. Resolved HIR symbols are intentionally ignored here because
///   receiver methods also appear in the backend receiver-first symbol table;
///   those are not separate source-level function declarations.
fn local_function_pipe_target_matches(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    if let Some(schemes) = ctx
        .signatures
        .get(&(function_name.to_string(), arg_types.len()))
    {
        let mut trial_subst = subst.clone();
        if infer_function_scheme_overload(schemes, function_name, arg_types, ctx, &mut trial_subst)
            .is_ok()
        {
            return true;
        }
    }

    false
}

/// Returns whether a selected imported function can accept pipe insertion.
///
/// Inputs:
/// - `function_name`: local selected-import name.
/// - `arg_types`: pipe-inserted argument types, including the receiver/input as
///   the first argument.
/// - `ctx` and `subst`: active inference context and current substitutions.
///
/// Output:
/// - `true` when the selected import resolves to a provider signature that can
///   accept the pipe-inserted arguments.
/// - `false` when the name is not a selected import, the provider interface is
///   unavailable, the arity is missing, or the arguments do not match.
///
/// Transformation:
/// - Resolves the local selected-import target through loaded interfaces and
///   checks the provider function scheme with cloned substitutions so ambiguity
///   detection does not mutate inference state or emit import diagnostics.
fn imported_function_pipe_target_matches(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let Some(target) = ctx.function_imports.get(function_name) else {
        return false;
    };
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        return false;
    };
    infer_imported_function_candidate_matches(
        interface,
        &target.function,
        function_name,
        arg_types,
        ctx,
        subst,
    )
}

/// Infers pipe-forward syntax that targets a receiver method.
///
/// Inputs:
/// - `left`: pipe input expression used as the receiver.
/// - `right`: call expression written as `method(args...)`.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the right side names a receiver method for that arity.
/// - `None` when no receiver-method candidate exists, allowing ordinary pipe
///   insertion to run.
///
/// Transformation:
/// - Resolves `value |> method(args)` as `value.method(args)` before ordinary
///   function insertion. Immutable receiver methods return their declared
///   return type. Mutable receiver methods return the updated receiver type for
///   pipe continuation, regardless of the command method's declared result.
fn infer_syntax_receiver_method_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if right.remote.is_some() || !syntax_callee_is_var(right) {
        return None;
    }

    let method = syntax_callee_name(right)?;
    let arity = right.children.len().saturating_sub(1);
    let candidates = ctx.receiver_methods.get(&(method.to_string(), arity))?;
    let receiver_type = infer_syntax_expr(left, locals, ctx, subst, errors);
    let arg_types = right
        .children
        .iter()
        .skip(1)
        .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors))
        .collect::<Vec<_>>();
    let mut pipe_inserted_arg_types = Vec::with_capacity(arg_types.len() + 1);
    pipe_inserted_arg_types.push(receiver_type.clone());
    pipe_inserted_arg_types.extend(arg_types.iter().cloned());

    for candidate in candidates {
        let mut trial_subst = subst.clone();
        if unify(&candidate.receiver_type, &receiver_type, &mut trial_subst).is_err() {
            continue;
        }
        if local_function_pipe_target_matches(method, &pipe_inserted_arg_types, ctx, &trial_subst)
            || imported_function_pipe_target_matches(
                method,
                &pipe_inserted_arg_types,
                ctx,
                &trial_subst,
            )
        {
            errors.push(format!(
                "ambiguous pipe target `{}` / {}: receiver method and ordinary function both match; use explicit receiver or function call syntax",
                method,
                arity
            ));
            return Some(Type::Dynamic);
        }
        match infer_function_with_bounds(
            &candidate.scheme,
            Some(method),
            &arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                let pipe_type = if candidate.receiver_mutable {
                    apply_subst(&receiver_type, &trial_subst)
                } else {
                    ty
                };
                *subst = trial_subst;
                return Some(pipe_type);
            }
            Err(message) => {
                errors.push(message);
                return Some(Type::Dynamic);
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arity,
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Infers a pipe-forwarding expression.
///
/// Inputs:
/// - `expr`: syntax-output pipe expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Return type of the right-side call after inserting the left value.
///
/// Transformation:
/// - Validates pipe shape, handles mutable receiver pipe forwarding, and
///   rewrites ordinary pipes to call inference with the left value prepended.
pub(super) fn infer_syntax_pipe_forward(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(left) = expr.children.first() else {
        return Type::Dynamic;
    };
    let Some(right) = expr.children.get(1) else {
        return Type::Dynamic;
    };
    if !matches!(
        right.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) {
        errors.push("right side of |> must be a function call".to_string());
        let _ = infer_syntax_expr(left, locals, ctx, subst, errors);
        let _ = infer_syntax_expr(right, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    if right.kind == SyntaxExprKind::Call {
        if let Some(ty) =
            infer_syntax_receiver_method_pipe_forward(left, right, locals, ctx, subst, errors)
        {
            return ty;
        }
    }

    let mut arg_types = Vec::with_capacity(right.children.len());
    arg_types.push(infer_syntax_expr(left, locals, ctx, subst, errors));
    arg_types.extend(
        right
            .children
            .iter()
            .skip(1)
            .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors)),
    );

    match right.kind {
        SyntaxExprKind::FunctionCall => infer_syntax_function_value_call_with_arg_types(
            right, &arg_types, locals, ctx, subst, errors,
        ),
        _ => infer_syntax_call_with_arg_types(right, &arg_types, locals, ctx, subst, errors),
    }
}

/// Extracts the source-visible callee name from a call expression.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - Callee text when the call head is a variable or atom.
///
/// Transformation:
/// - Reads the first call child and returns its preserved text only for
///   name-like callee nodes.
pub(crate) fn syntax_callee_name(expr: &SyntaxExprOutput) -> Option<&str> {
    expr.children.first().and_then(|callee| match callee.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => callee.text.as_deref(),
        _ => None,
    })
}

/// Checks whether a call expression's callee is a variable node.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
///
/// Output:
/// - `true` when the first child is a variable callee.
///
/// Transformation:
/// - Examines only the call head shape without resolving the identifier.
pub(super) fn syntax_callee_is_var(expr: &SyntaxExprOutput) -> bool {
    matches!(
        expr.children.first().map(|callee| callee.kind),
        Some(SyntaxExprKind::Var)
    )
}

/// Infers an explicit remote call.
///
/// Inputs:
/// - `expr`: call expression with a remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved remote call return type.
///
/// Transformation:
/// - Resolves imported modules, trait calls, target intrinsics, and interface
///   functions before falling back to dynamic typing with diagnostics.
fn infer_syntax_remote_call(
    module_name: &str,
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let resolved_module_name = ctx
        .module_aliases
        .get(module_name)
        .map(String::as_str)
        .unwrap_or(module_name);

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
        let cached_lookup_arg_types = canonicalize_trait_lookup_types(lookup_arg_types.as_slice());
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
                    if !trait_method_candidate_matches_call(
                        impl_candidate,
                        &lookup_arg_types,
                        ctx,
                        subst,
                    ) {
                        continue;
                    }
                    let mut trial_subst = subst.clone();
                    if infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
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
                    if let Ok(ty) = infer_function_call(
                        &impl_candidate.scheme,
                        &lookup_arg_types,
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
        match infer_interface_function_overload(
            interface,
            function_name,
            &format!("{}.{}", resolved_module_name, function_name),
            arg_types,
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
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
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
            if let Some(constructed) =
                infer_constructor_schemes(function_name, &schemes, arg_types, subst, errors)
            {
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

    Type::Dynamic
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
/// - `true` when the candidate has no owner type information or when its first
///   impl type argument unifies with the call's first argument type.
/// - `false` when a different concrete conformance owns the method.
///
/// Transformation:
/// - Uses a cloned substitution table and transparent alias expansion to filter
///   trait method candidates before ambiguity counting. This keeps imported
///   multi-conformance traits such as `std.core.String.Show` from treating
///   `Show[Int]`, `Show[Bool]`, and `Show[String]` as simultaneous matches for
///   one receiver/value argument.
pub(super) fn trait_method_candidate_matches_call(
    candidate: &ResolvedTraitMethod,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
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
    unify(&owner_expanded, &arg_expanded, &mut trial_subst).is_ok()
}

/// Infers a local receiver-method call.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for a resolved local receiver method or a method-shaped call
///   that has candidates but no matching receiver.
/// - `None` when the expression is not a receiver-method call known to the
///   current module.
///
/// Transformation:
/// - Reads `receiver.method(args...)` from the field-access callee, infers the
///   receiver type, selects a receiver-method signature by method/arity and
///   receiver unification, then checks the non-receiver arguments with the
///   existing function-scheme inference path.
fn infer_syntax_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let candidates = ctx
        .receiver_methods
        .get(&(method.to_string(), arg_types.len()))?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);

    for candidate in candidates {
        let mut trial_subst = subst.clone();
        if unify(&candidate.receiver_type, &receiver_type, &mut trial_subst).is_err() {
            continue;
        }
        match infer_function_with_bounds(
            &candidate.scheme,
            Some(method),
            arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(message) => {
                errors.push(message);
                return Some(Type::Dynamic);
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arg_types.len(),
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Infers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for supported primitive receiver calls.
/// - `None` when the expression is not a supported primitive receiver call.
///
/// Transformation:
/// - Reads the receiver type from the field-access callee, prepends that type to
///   the argument check, validates the primitive method's arity and parameter
///   types, and returns the method result type.
fn infer_syntax_primitive_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);
    let scheme = primitive_receiver_method_scheme(&receiver_type, method, arg_types.len())?;
    infer_function_with_bounds(&scheme, Some(method), arg_types, ctx, subst)
        .map(Some)
        .unwrap_or_else(|message| {
            errors.push(message);
            Some(Type::Dynamic)
        })
}

/// Unwraps macro-specific return wrappers.
///
/// Inputs:
/// - `ty`: inferred macro implementation return type.
///
/// Output:
/// - User-visible macro expansion result type.
///
/// Transformation:
/// - Removes one known macro wrapper layer and leaves all other types
///   unchanged.
fn unwrap_macro_return_type(ty: Type) -> Type {
    match ty {
        Type::Named {
            module,
            name: tag,
            args,
        } if module.is_none() && tag == "Ast" && args.len() == 1 => {
            args.into_iter().next().unwrap_or(Type::Dynamic)
        }
        other => other,
    }
}

/// Infers a local named call.
///
/// Inputs:
/// - `expr`: call expression without an explicit remote qualifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Resolved local call return type.
///
/// Transformation:
/// - Checks constructors, local functions, imports, aliases, trait shorthands,
///   receiver forms, and intrinsics in source-call priority order.
fn infer_syntax_local_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    if is_removed_implicit_builtin_call(function_name, arg_types.len()) {
        errors.push(format!(
            "`{function_name}/{}` is not part of the implicit prelude; import or define it explicitly",
            arg_types.len()
        ));
        return Type::Dynamic;
    }

    if let Some(scheme) = builtin_call(function_name, arg_types.len()) {
        if let Err(message) =
            infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst)
        {
            errors.push(message);
        }
        return scheme.ret;
    }

    if let Some(ty) =
        infer_syntax_imported_function_call(function_name, arg_types, ctx, subst, errors)
    {
        return ty;
    }

    if let Some(schemes) = ctx
        .signatures
        .get(&(function_name.to_string(), arg_types.len()))
    {
        match infer_function_scheme_overload(schemes, function_name, arg_types, ctx, subst) {
            Ok(ty) => return ty,
            Err(message) => {
                errors.push(message);
                return Type::Dynamic;
            }
        }
    }

    if let Some(symbol) = ctx
        .local_fns
        .get(&(function_name.to_string(), arg_types.len()))
    {
        if let Some(scheme) = parse_symbol_scheme(symbol) {
            match infer_function_with_bounds(&scheme, Some(function_name), arg_types, ctx, subst) {
                Ok(ty) => return ty,
                Err(message) => {
                    errors.push(message);
                    return Type::Dynamic;
                }
            }
        }
    }

    Type::Dynamic
}

/// Infers a selected imported function call.
///
/// Inputs:
/// - `function_name`: local call name from source, possibly an import alias.
/// - `arg_types`: already inferred argument types.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the local name is a selected function import.
/// - `None` when the local name is not imported as a function.
///
/// Transformation:
/// - Resolves the local import target to its provider module interface, parses
///   the public function signature for the call arity, and reuses ordinary
///   function-call inference so argument mismatches are reported before backend
///   emission.
fn infer_syntax_imported_function_call(
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let target = ctx.function_imports.get(function_name)?;
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_interface_message(
                resolved_module,
                &target.function,
                ctx.interface_map,
            ),
        ));
        return Some(Type::Dynamic);
    };

    match infer_interface_function_overload(
        interface,
        &target.function,
        function_name,
        arg_types,
        ctx,
        subst,
    ) {
        Ok(Some(ty)) => return Some(ty),
        Ok(None) => {}
        Err(message) => {
            errors.push(spanned_expression_error(target.span, message));
            return Some(Type::Dynamic);
        }
    }

    if !interface
        .functions
        .contains_key(&(target.function.clone(), arg_types.len()))
        && !interface
            .function_overloads
            .contains_key(&(target.function.clone(), arg_types.len()))
    {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_message(interface, &target.function, arg_types.len()),
        ));
        return Some(Type::Dynamic);
    }

    Some(Type::Dynamic)
}
