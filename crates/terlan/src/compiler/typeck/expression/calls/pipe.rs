use super::*;

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
    let Some(targets) = ctx.function_imports.get(function_name) else {
        return false;
    };
    targets.iter().any(|target| {
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
    })
}

/// Returns whether ordinary pipe insertion has a declared target at this arity.
fn ordinary_function_pipe_target_exists(
    function_name: &str,
    arity: usize,
    ctx: &ExprInferContext,
) -> bool {
    if ctx
        .signatures
        .contains_key(&(function_name.to_string(), arity))
    {
        return true;
    }

    ctx.function_imports.contains_key(function_name)
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

    if ordinary_function_pipe_target_exists(method, pipe_inserted_arg_types.len(), ctx) {
        return None;
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

/// Returns the module alias from a field-access callee.
///
/// Inputs:
/// - `expr`: syntax-output field access such as `Module.member`.
///
/// Output:
/// - The source receiver name when it is a module-shaped name.
///
/// Transformation:
/// - Accepts both atom and variable receiver nodes because uppercase module
///   aliases are represented as name-like atoms in syntax output.
fn syntax_module_member_receiver_name(expr: &SyntaxExprOutput) -> Option<&str> {
    let receiver = expr.children.first()?;
    match receiver.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => receiver.text.as_deref(),
        _ => None,
    }
}

/// Infers pipe-forward syntax targeting an imported module member.
///
/// Inputs:
/// - `left`: pipe input expression inserted as the first call argument.
/// - `right`: call expression whose callee may be `Module.function`.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference
///   state.
///
/// Output:
/// - `Some(Type)` when the right-side callee is an imported module member.
/// - `None` when the right side is not an imported module-member call.
///
/// Transformation:
/// - Converts `value |> Module.function(args...)` into the same typechecking
///   shape as `Module.function(value, args...)`, preserving explicit arguments
///   and type arguments on the pipe stage.
fn infer_syntax_imported_module_member_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let (module_alias, member) = if let Some(remote) = right.remote.as_deref() {
        (remote, syntax_callee_name(right)?)
    } else {
        let callee = right.children.first()?;
        if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
            return None;
        }
        (
            syntax_module_member_receiver_name(callee)?,
            callee.text.as_deref()?,
        )
    };
    let display_name = format!("{}.{}", module_alias, member);
    let mut arg_types = Vec::with_capacity(right.children.len());
    arg_types.push(infer_syntax_expr(left, locals, ctx, subst, errors));
    arg_types.extend(
        right
            .children
            .iter()
            .skip(1)
            .map(|arg| infer_syntax_expr(arg, locals, ctx, subst, errors)),
    );
    let mut arg_names = Vec::with_capacity(right.arg_names.len() + 1);
    arg_names.push(None);
    arg_names.extend(right.arg_names.iter().cloned());

    infer_syntax_imported_module_member_function_call(
        module_alias,
        member,
        &display_name,
        SyntaxCallArguments {
            types: &arg_types,
            type_args: &right.type_args,
            names: &arg_names,
        },
        ctx,
        subst,
        errors,
    )
}

/// Infers pipe-forward syntax targeting a selected imported function.
fn infer_syntax_selected_import_pipe_forward(
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
    let function_name = syntax_callee_name(right)?;
    if !ctx.function_imports.contains_key(function_name) {
        return None;
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
    let mut arg_names = Vec::with_capacity(right.arg_names.len() + 1);
    arg_names.push(None);
    arg_names.extend(right.arg_names.iter().cloned());

    if !imported_function_pipe_target_matches(function_name, &arg_types, ctx, subst) {
        return None;
    }

    infer_syntax_imported_function_call(
        function_name,
        &arg_types,
        &right.type_args,
        &arg_names,
        ctx,
        subst,
        errors,
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
pub(crate) fn infer_syntax_pipe_forward(
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
        if let Some(ty) =
            infer_syntax_selected_import_pipe_forward(left, right, locals, ctx, subst, errors)
        {
            return ty;
        }
        if let Some(ty) = infer_syntax_imported_module_member_pipe_forward(
            left, right, locals, ctx, subst, errors,
        ) {
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
