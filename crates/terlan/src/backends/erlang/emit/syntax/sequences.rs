use super::*;

/// Lowers a semicolon-style expression sequence.
///
/// Inputs:
/// - `expr`: syntax-output sequence expression.
/// - `ctx`: syntax lowering context with receiver-method and intrinsic data.
/// - `env`: current expression lowering environment.
///
/// Output:
/// - Lowered Erlang expression for the sequence.
/// - `None` when the sequence is empty or any child cannot lower.
///
/// Transformation:
/// - Evaluates children left-to-right. Non-final ordinary expressions are bound
///   to ignored temporaries to preserve effects. Non-final mutable receiver
///   calls bind the hidden backend-updated receiver and update the local
///   replacement environment so later source references to the receiver lower
///   to that updated binding.
pub(super) fn lower_syntax_sequence_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let (last, prefix) = expr.children.split_last()?;
    let mut sequence_env = env.clone();
    let mut bindings = Vec::new();

    for (index, child) in prefix.iter().enumerate() {
        if let Some((receiver_name, binding)) =
            lower_syntax_mutable_update_binding(child, ctx, &sequence_env, index)
        {
            let binding_name = erl_let_binding_var_name(&binding)?;
            sequence_env
                .value_replacements
                .insert(receiver_name, ErlExpr::Var(binding_name.to_string()));
            bindings.push(binding);
        } else {
            bindings.push(ErlLetBinding {
                pattern: ErlPattern::Var(format!("_TerlanSeqIgnored{index}")),
                value: lower_syntax_expr_with_env(child, ctx, &sequence_env)?,
            });
        }
    }

    let body = if let Some((receiver_name, binding)) =
        lower_syntax_mutable_update_binding(last, ctx, &sequence_env, bindings.len())
    {
        let updated_receiver = ErlExpr::Var(erl_let_binding_var_name(&binding)?.to_string());
        sequence_env
            .value_replacements
            .insert(receiver_name, updated_receiver.clone());
        bindings.push(binding);
        updated_receiver
    } else {
        lower_syntax_expr_with_env(last, ctx, &sequence_env)?
    };

    if bindings.is_empty() {
        Some(body)
    } else {
        Some(ErlExpr::Let {
            bindings,
            body: Box::new(body),
        })
    }
}

/// Builds one mutable update binding from supported source mutation syntax.
///
/// Inputs:
/// - `expr`: syntax-output expression that may update a mutable receiver.
/// - `ctx`, `env`: lowering context and current replacement-aware environment.
/// - `index`: deterministic sequence-local temporary index.
///
/// Output:
/// - Receiver source name and Erlang binding for the backend-updated receiver.
/// - `None` for expressions outside the supported mutation surface.
///
/// Transformation:
/// - Routes direct mutable receiver calls and indexed assignment through their
///   dedicated recognizers so sequence lowering can rebind the updated receiver
///   consistently regardless of source spelling.
fn lower_syntax_mutable_update_binding(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
    index: usize,
) -> Option<(String, ErlLetBinding)> {
    lower_syntax_mutable_receiver_update_binding(expr, ctx, env, index)
        .or_else(|| lower_syntax_index_assign_update_binding(expr, ctx, env, index))
}

/// Builds one mutable receiver update binding from a direct method call.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be `receiver.method(args...)`.
/// - `ctx`, `env`: lowering context and current replacement-aware environment.
/// - `index`: deterministic sequence-local temporary index.
///
/// Output:
/// - Receiver source name and Erlang binding for the backend-updated receiver.
/// - `None` for non-call expressions, non-variable receivers, immutable
///   methods, or unsupported child expressions.
///
/// Transformation:
/// - Recognizes a direct mutable receiver call, lowers the call through the
///   backend receiver-first convention, and captures its hidden updated
///   receiver result in a deterministic temporary variable.
fn lower_syntax_mutable_receiver_update_binding(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
    index: usize,
) -> Option<(String, ErlLetBinding)> {
    if !matches!(expr.kind, SyntaxExprKind::Call) || expr.remote.is_some() {
        return None;
    }

    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }

    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    if !matches!(receiver.kind, SyntaxExprKind::Var) {
        return None;
    }
    let receiver_name = receiver.text.clone()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, ctx, env)?;
    let arity = expr.children.len().checked_sub(1)?;
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity);
    let declared_receiver_target = receiver_target.filter(|target| target.mutable);
    let receiver_target_mutable = declared_receiver_target.is_some()
        || is_mutating_primitive_receiver_method(&receiver_type, method, arity);
    let receiver_target_mutable = receiver_target_mutable
        || is_http_response_mutating_receiver_method(&receiver_type, method, arity);
    let receiver_target_mutable = receiver_target_mutable
        || is_native_vector_mutating_receiver_method(&receiver_type, method, arity);
    if !receiver_target_mutable {
        return None;
    }

    let args = &expr.children[1..];
    if let Some(value) = lower_http_response_receiver_method_call(
        &receiver_type,
        method,
        receiver,
        args,
        &expr.arg_names,
        ctx,
        env,
    ) {
        return Some((
            receiver_name,
            ErlLetBinding {
                pattern: ErlPattern::Var(format!("_TerlanMutReceiver{index}")),
                value,
            },
        ));
    }

    if let Some(value) =
        lower_syntax_native_vector_receiver_call(&receiver_type, method, receiver, args, ctx, env)
    {
        return Some((
            receiver_name,
            ErlLetBinding {
                pattern: ErlPattern::Var(format!("_TerlanMutReceiver{index}")),
                value,
            },
        ));
    }

    let mut lowered_args = Vec::with_capacity(
        declared_receiver_target
            .map(|target| target.fixed_arity)
            .unwrap_or(arity)
            + 1,
    );
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    if let Some(target) = declared_receiver_target {
        lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
            args,
            &expr.arg_names,
            target,
            ctx,
            env,
        )?);
    } else {
        lowered_args.extend(
            args.iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        );
    }

    let value = match primitive_receiver_method_intrinsic(&receiver_type, method, arity) {
        Some(intrinsic) => lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)?,
        None => ErlExpr::Call {
            module: None,
            function: method.to_string(),
            args: lowered_args,
        },
    };

    Some((
        receiver_name,
        ErlLetBinding {
            pattern: ErlPattern::Var(format!("_TerlanMutReceiver{index}")),
            value,
        },
    ))
}

/// Builds one mutable receiver update binding from indexed assignment syntax.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be `collection[index] = value`.
/// - `ctx`, `env`: lowering context and current replacement-aware environment.
/// - `index`: deterministic sequence-local temporary index.
///
/// Output:
/// - Collection source name and Erlang binding for the backend-updated
///   collection.
/// - `None` for non-index-assignment expressions, non-variable collections,
///   immutable/nonexistent `set_at` receiver methods, or unsupported children.
///
/// Transformation:
/// - Lowers indexed assignment through the receiver-first `set_at` ABI used by
///   command-style mutable receiver methods. The binding value is the updated
///   collection, and sequence lowering installs it as the replacement for later
///   references to the same source name.
fn lower_syntax_index_assign_update_binding(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
    index: usize,
) -> Option<(String, ErlLetBinding)> {
    if !matches!(expr.kind, SyntaxExprKind::IndexAssign) || expr.children.len() != 3 {
        return None;
    }

    let collection = expr.children.first()?;
    if !matches!(collection.kind, SyntaxExprKind::Var) {
        return None;
    }
    let collection_name = collection.text.clone()?;
    let value = lower_syntax_index_assign_update_call(expr, ctx, env)?;

    Some((
        collection_name,
        ErlLetBinding {
            pattern: ErlPattern::Var(format!("_TerlanMutReceiver{index}")),
            value,
        },
    ))
}

/// Returns the Erlang variable name bound by a sequence temporary.
///
/// Inputs:
/// - `binding`: sequence-local let binding created by mutable update lowering.
///
/// Output:
/// - Variable name when the binding pattern is a simple variable.
/// - `None` for non-variable patterns, which sequence mutation lowering never
///   intentionally creates.
///
/// Transformation:
/// - Keeps sequence state replacement separate from the more general
///   pattern-capable `ErlLetBinding` render model.
fn erl_let_binding_var_name(binding: &ErlLetBinding) -> Option<&str> {
    match &binding.pattern {
        ErlPattern::Var(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Returns whether a primitive receiver method updates its receiver binding.
///
/// Inputs:
/// - `receiver_type`: inferred source type of the receiver expression.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver call arguments.
///
/// Output:
/// - `true` for compiler-owned command-style collection mutators.
/// - `false` for observers, pure primitive methods, and unsupported calls.
///
/// Transformation:
/// - Extracts the nominal collection type head and matches only the selected
///   0.0.2 mutable receiver ABI methods so sequence lowering can rebind
///   imported std collection receivers without requiring local method bodies.
fn is_mutating_primitive_receiver_method(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> bool {
    if receiver_type_has_head(receiver_type, "std.beam.Agent.Agent") {
        return matches!(
            (method, arg_count),
            ("update", 1) | ("cast", 1) | ("stop", 0)
        );
    }

    if receiver_type_has_head(receiver_type, "std.beam.Task.Task") {
        return matches!((method, arg_count), ("cancel", 0));
    }

    if receiver_type_has_head(receiver_type, "std.beam.GenServer.ServerRef") {
        return matches!((method, arg_count), ("cast", 1) | ("stop", 0));
    }

    matches!(
        (
            receiver_type_head(receiver_type).as_str(),
            method,
            arg_count
        ),
        ("List", "push", 1)
            | ("List", "clear", 0)
            | ("Map", "put", 2)
            | ("Map", "remove", 1)
            | ("Map", "clear", 0)
            | ("Object", "put", 2)
            | ("Object", "remove", 1)
            | ("Object", "clear", 0)
            | ("Set", "add", 1)
            | ("Set", "remove", 1)
            | ("Set", "clear", 0)
    )
}
