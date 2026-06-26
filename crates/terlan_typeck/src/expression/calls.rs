use super::*;
mod imported;
mod local;
mod receiver;
mod template;

use imported::*;
use local::*;
use receiver::*;
use template::*;

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
    let arg_types = infer_syntax_call_arg_types(expr, locals, ctx, subst, errors);
    infer_syntax_call_with_arg_types(expr, &arg_types, locals, ctx, subst, errors)
}

/// Infers call argument types with available local-call context.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Argument types in source order.
///
/// Transformation:
/// - For ordinary local calls with a known exact signature, supplies each
///   argument's expected parameter type to contextual expressions such as
///   `Module.member` function values. All other calls use ordinary expression
///   inference.
fn infer_syntax_call_arg_types(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Vec<Type> {
    let expected_arg_types = exact_local_call_expected_arg_types(expr, ctx, subst);
    expr.children
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, arg)| {
            expected_arg_types
                .as_ref()
                .and_then(|expected| expected.get(index))
                .and_then(Option::as_ref)
                .and_then(|expected| {
                    infer_syntax_expr_with_expected(arg, expected, locals, ctx, subst, errors)
                })
                .unwrap_or_else(|| infer_syntax_expr(arg, locals, ctx, subst, errors))
        })
        .collect()
}

/// Builds positional expected argument types for an exact local function call.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `ctx`: expression inference context containing local function signatures.
/// - `subst`: active substitution table used when instantiating generic
///   function schemes.
///
/// Output:
/// - Expected source-argument types when the call head is a direct local
///   function and the supplied arity exactly matches a known declaration.
/// - `None` for remote calls, receiver calls, constructors, imports, or calls
///   whose local signature is not exact.
///
/// Transformation:
/// - Parses the local function scheme, instantiates generic variables, and
///   maps named arguments back to declaration slots without changing source
///   argument order.
fn exact_local_call_expected_arg_types(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Vec<Option<Type>>> {
    if expr.remote.is_some() || !syntax_callee_is_var(expr) {
        return None;
    }
    let function_name = syntax_callee_name(expr)?;
    let supplied_arity = expr.children.len().saturating_sub(1);
    let symbol = ctx
        .local_fns
        .get(&(function_name.to_string(), supplied_arity))?;
    let scheme = parse_symbol_scheme(symbol)?;
    let instantiated =
        instantiate_function_scheme_from(&scheme, next_function_type_var(&[], subst));
    let param_names = symbol
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<Vec<_>>();

    let mut next_positional = 0;
    Some(
        expr.arg_names
            .iter()
            .map(|arg_name| {
                let slot = if let Some(arg_name) = arg_name {
                    param_names.iter().position(|param| param == arg_name)
                } else {
                    while next_positional < param_names.len()
                        && expr
                            .arg_names
                            .iter()
                            .any(|name| name.as_deref() == Some(param_names[next_positional]))
                    {
                        next_positional += 1;
                    }
                    let slot = (next_positional < param_names.len()).then_some(next_positional);
                    next_positional += 1;
                    slot
                }?;
                instantiated.params.get(slot).cloned()
            })
            .collect(),
    )
}

/// Infers one expression with a contextual expected type when supported.
///
/// Inputs:
/// - `expr`: argument expression.
/// - `expected`: expected parameter type from the receiving call.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - Contextually inferred type for supported forms.
/// - `None` when the expression has no contextual inference behavior.
///
/// Transformation:
/// - Currently uses function-value expectations to resolve overloaded
///   imported module-member references such as `Users.index`.
pub(crate) fn infer_syntax_expr_with_expected(
    expr: &SyntaxExprOutput,
    expected: &Type,
    _locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    infer_imported_module_member_function_value_with_expected(expr, expected, ctx, subst, errors)
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
/// - Infers the callee expression, requires a `Type::Function`, checks each
///   provided argument against the corresponding parameter with alias-aware
///   subtyping before falling back to unification, and returns the substituted
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
                let expected_substituted = apply_subst(expected, subst);
                let actual_substituted = apply_subst(actual, subst);
                if is_subtype_with_aliases(&actual_substituted, &expected_substituted, ctx.aliases)
                {
                    continue;
                }
                if let Err(original_message) = unify(expected, actual, subst) {
                    let expected_expanded = expand_type_aliases(&expected_substituted, ctx.aliases);
                    let actual_expanded = expand_type_aliases(&actual_substituted, ctx.aliases);
                    if is_subtype_with_aliases(&actual_expanded, &expected_expanded, ctx.aliases) {
                        continue;
                    }
                    if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                        errors.push(original_message);
                    }
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
        if let Some(template_result) =
            infer_syntax_template_call(function_name, expr, locals, ctx, subst, errors)
        {
            return template_result;
        }

        if ctx.current_constructor_target == Some(function_name) {
            if let Some(constructed) = infer_default_struct_constructor_call(
                function_name,
                &arg_types,
                &expr.arg_names,
                ctx,
                subst,
                errors,
            ) {
                return constructed;
            }
        }

        if let Some(constructed) = infer_constructor_call(
            function_name,
            &arg_types,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        ) {
            return constructed;
        }

        if let Some(imported) = ctx.constructor_aliases.get(function_name) {
            if let Some(interface) = ctx.interface_map.get(&imported.module) {
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
                        &expr.arg_names,
                        subst,
                        errors,
                    ) {
                        let interface_aliases = interface_type_aliases(interface);
                        return expand_type_aliases(&constructed, &interface_aliases);
                    }
                }
                if interface.opaque_types.contains(&imported.name) {
                    errors.push(format!(
                        "cannot construct opaque type {}.{} outside defining module",
                        imported.module, imported.name
                    ));
                    return Type::Dynamic;
                }
            }
        }

        if let Some(constructed) = infer_default_struct_constructor_call(
            function_name,
            &arg_types,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        ) {
            return constructed;
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
            let diagnostic_span = expr
                .children
                .first()
                .map(|callee| callee.span.into())
                .unwrap_or_else(|| expr.span.into());
            errors.push(spanned_expression_error(
                diagnostic_span,
                format!(
                    "unknown constructor {} / {}",
                    function_name,
                    arg_types.len()
                ),
            ));
            return Type::Dynamic;
        }
    }

    if let Some(module_name) = expr.remote.as_deref() {
        return infer_syntax_remote_call(
            module_name,
            function_name,
            arg_types,
            &expr.type_args,
            &expr.arg_names,
            ctx,
            subst,
            errors,
        );
    }

    infer_syntax_local_call(
        function_name,
        arg_types,
        &expr.type_args,
        &expr.arg_names,
        ctx,
        subst,
        errors,
    )
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
    let candidates = receiver_method_candidates_accepting_call(ctx, method, arity)?;
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

    for candidate in &candidates {
        let mut trial_subst = subst.clone();
        let param_names = candidate
            .param_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !validate_named_call_args(method, &right.arg_names, &param_names, errors) {
            return Some(Type::Dynamic);
        }
        if !validate_required_defaulted_receiver_call_args(
            method,
            &right.arg_names,
            candidate,
            errors,
        ) {
            return Some(Type::Dynamic);
        }
        let effective_arg_types = complete_defaulted_receiver_call_arg_types(
            &arg_types,
            &right.arg_names,
            &param_names,
            candidate,
        );
        let candidate_result = infer_receiver_method_candidate(
            candidate,
            Some(method),
            &receiver_type,
            &effective_arg_types,
            ctx,
            &mut trial_subst,
        );
        let Ok(ty) = candidate_result else {
            continue;
        };
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
        let pipe_type = if candidate.receiver_mutable {
            apply_subst(&receiver_type, &trial_subst)
        } else {
            ty
        };
        *subst = trial_subst;
        return Some(pipe_type);
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

/// Infers one receiver-method candidate with linked receiver generics.
///
/// Inputs:
/// - `candidate`: receiver dispatch candidate selected by method name/arity.
/// - `function_name`: optional diagnostic label for the method.
/// - `receiver_type`: inferred type of the receiver expression.
/// - `arg_types`: inferred non-receiver argument types after default
///   completion.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Candidate return type when the receiver and arguments satisfy the method
///   signature.
/// - Diagnostic text when this candidate does not accept the call.
///
/// Transformation:
/// - Builds a synthetic function scheme whose first parameter is the receiver
///   type, freshens that complete scheme once, then infers the call without a
///   second freshening step. This keeps receiver generics such as
///   `Map[K, V]` tied to method parameters and return types.
fn infer_receiver_method_candidate(
    candidate: &ReceiverMethodDispatchSignature,
    function_name: Option<&str>,
    receiver_type: &Type,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let candidate_receiver_type = qualify_imported_named_heads(&candidate.receiver_type, ctx);
    let receiver_type = qualify_imported_named_heads(receiver_type, ctx);
    let method_params = candidate
        .scheme
        .params
        .iter()
        .map(|param| qualify_imported_named_heads(param, ctx))
        .collect::<Vec<_>>();
    let method_return = qualify_imported_named_heads(&candidate.scheme.ret, ctx);
    let arg_types = arg_types
        .iter()
        .map(|arg| qualify_imported_named_heads(arg, ctx))
        .collect::<Vec<_>>();
    let mut params = Vec::with_capacity(arg_types.len() + 1);
    params.push(candidate_receiver_type);
    params.extend(method_params);
    let combined_scheme = FunctionScheme {
        params,
        ret: method_return,
        generic_params: candidate.scheme.generic_params.clone(),
        bounds: candidate.scheme.bounds.clone(),
    };
    let mut combined_args = Vec::with_capacity(arg_types.len() + 1);
    combined_args.push(receiver_type);
    combined_args.extend(arg_types);
    let instantiated = instantiate_function_scheme_from(
        &combined_scheme,
        next_function_type_var(&combined_args, subst),
    );

    infer_instantiated_function_with_bounds(
        &instantiated,
        function_name,
        &combined_args,
        ctx,
        subst,
    )
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
/// - `type_args`: explicit generic call arguments from `Module.fun[Type](...)`.
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

/// Infers the compiler-provided field-assignment constructor for a struct.
///
/// Inputs:
/// - `function_name`: source call head, expected to name a visible struct.
/// - `arg_types`: inferred argument types in source order.
/// - `arg_names`: field names supplied by named call arguments.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type::Named)` when `function_name` resolves to a visible struct.
/// - `None` when the call head is not a struct name.
///
/// Transformation:
/// - Treats `User(name = value)` as the default struct constructor, requiring
///   explicit field assignments, rejecting unknown/duplicate/missing fields,
///   enforcing field visibility, and unifying each supplied value with the
///   declared field type.
fn infer_default_struct_constructor_call(
    function_name: &str,
    arg_types: &[Type],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let field_types = ctx.struct_fields.get(function_name)?;
    if ctx.constructors.contains_key(function_name)
        && ctx.current_constructor_target != Some(function_name)
    {
        errors.push(format!(
            "implicit struct constructor `{}` is disabled because explicit constructors are declared",
            function_name
        ));
        return Some(Type::Dynamic);
    }
    let mut supplied = HashSet::new();

    for (index, actual) in arg_types.iter().enumerate() {
        let Some(source_field_name) = arg_names.get(index).and_then(Option::as_deref) else {
            errors.push(format!(
                "struct constructor `{}` requires named field arguments",
                function_name
            ));
            continue;
        };
        let (field_name, requested_private) = split_private_field_spelling(source_field_name);
        if !supplied.insert(field_name.to_string()) {
            errors.push(format!(
                "duplicate field `{}` in struct constructor `{}`",
                field_name, function_name
            ));
            continue;
        }

        let Some(expected) = field_types.get(field_name) else {
            errors.push(format!(
                "unknown field `{}` on struct `{}`",
                field_name, function_name
            ));
            continue;
        };
        if let Some(message) = struct_field_visibility_error(
            function_name,
            field_name,
            requested_private,
            ctx.struct_field_visibility,
            ctx.imported_type_names,
        ) {
            errors.push(message);
        }

        let expected_expanded = expand_type_aliases(expected, ctx.aliases);
        let actual_expanded = expand_type_aliases(actual, ctx.aliases);
        if let Err(message) = unify(&expected_expanded, &actual_expanded, subst) {
            errors.push(format!(
                "field `{}` on struct `{}` {}",
                field_name, function_name, message
            ));
        }
    }

    for field_name in field_types.keys() {
        if !supplied.contains(field_name) {
            errors.push(format!(
                "missing field `{}` in struct constructor `{}`",
                field_name, function_name
            ));
        }
    }

    Some(Type::Named {
        module: None,
        name: function_name.to_string(),
        args: Vec::new(),
    })
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
pub(super) fn trait_method_candidate_matches_call(
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
fn qualify_imported_named_heads(ty: &Type, ctx: &ExprInferContext<'_>) -> Type {
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
                .map(|field| super::MapFieldType {
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
