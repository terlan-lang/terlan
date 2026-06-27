use super::*;

/// Lowers a pipe-forwarding binary expression.
///
/// Inputs:
/// - `left`: expression whose value flows into the right-hand call.
/// - `right`: pipe target expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Lowered call expression with `left` inserted as the first argument.
/// - Raw fallback expression when the right side is not a call.
///
/// Transformation:
/// - Preserves mutable receiver pipe forwarding as a special case, otherwise
///   rewrites `left |> f(args...)` to the same call router as `f(left, args...)`
///   so ordinary calls, trait calls, and imports share behavior.
pub(in crate::backends::erlang::emit::syntax) fn lower_syntax_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) {
        return Some(ErlExpr::Raw(format!(
            "{} |> {}",
            lower_syntax_expr_with_env(left, ctx, env)?.render(),
            lower_syntax_expr_with_env(right, ctx, env)?.render()
        )));
    }

    if let Some(expr) = lower_syntax_mutable_receiver_pipe_forward(left, right, ctx, env) {
        return Some(expr);
    }
    if let Some(expr) = lower_syntax_receiver_method_pipe_forward(left, right, ctx, env) {
        return Some(expr);
    }

    let callee = right.children.first()?;
    let mut args = Vec::with_capacity(right.children.len());
    args.push(left.clone());
    args.extend(right.children.iter().skip(1).cloned());
    lower_syntax_call_parts(callee, &args, &[], right.remote.as_deref(), ctx, env)
}

/// Lowers immutable receiver-method pipe forwarding.
///
/// Inputs:
/// - `left`: source pipe receiver expression.
/// - `right`: source call expression on the right side of `|>`.
/// - `ctx`, `env`: syntax lowering context and lexical type environment.
///
/// Output:
/// - `Some(ErlExpr::Call)` when `right` names a declared immutable receiver
///   method for the inferred receiver type.
/// - `None` for mutable receiver methods and non-receiver pipe targets.
///
/// Transformation:
/// - Rewrites `receiver |> method(args...)` into the receiver-first backend
///   call shape and fills omitted receiver-method default arguments.
fn lower_syntax_receiver_method_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) || right.remote.is_some() {
        return None;
    }

    let callee = right.children.first()?;
    let method = syntax_expr_name(callee)?;
    let arity = right.children.len().checked_sub(1)?;
    let receiver_type = infer_syntax_pipe_receiver_type(left, ctx, env)?;
    let args = &right.children[1..];
    if let Some(expr) =
        lower_syntax_native_vector_receiver_call(&receiver_type, method, left, args, ctx, env)
    {
        return Some(expr);
    }
    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity)?;
    if receiver_target.mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(receiver_target.fixed_arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(left, ctx, env)?);
    lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
        args,
        &right.arg_names,
        receiver_target,
        ctx,
        env,
    )?);

    Some(ErlExpr::Call {
        module: None,
        function: method.to_string(),
        args: lowered_args,
    })
}

/// Lowers mutable receiver-method pipe forwarding.
///
/// Inputs:
/// - `left`: source pipe receiver expression.
/// - `right`: source call expression on the right side of `|>`.
/// - `ctx`, `env`: syntax lowering context and lexical type environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when `right` names a declared mutable receiver method
///   for the inferred receiver type or a compiler-known direct HTTP response
///   helper.
/// - `None` for non-call right sides, remote calls, non-method calls,
///   immutable receiver methods, or expressions whose receiver type cannot be
///   inferred from the syntax lowering environment.
///
/// Transformation:
/// - Rewrites `receiver |> mut_method(args...)` into a backend-local binding
///   whose value is the hidden updated receiver returned by the lowered mutable
///   method function. The binding result becomes the pipe expression value so
///   later pipe steps receive the updated receiver. Direct HTTP response
///   helpers use the same pipe result shape because the BEAM handler bridge
///   represents response edits as updated response tuples.
fn lower_syntax_mutable_receiver_pipe_forward(
    left: &SyntaxExprOutput,
    right: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(right.kind, SyntaxExprKind::Call) || right.remote.is_some() {
        return None;
    }

    let callee = right.children.first()?;
    let method = syntax_expr_name(callee)?;
    let arity = right.children.len().checked_sub(1)?;
    let receiver_type = infer_syntax_pipe_receiver_type(left, ctx, env)?;
    let args = &right.children[1..];
    if is_native_vector_mutating_receiver_method(&receiver_type, method, arity) {
        let updated_receiver = "_TerlanMutReceiver".to_string();
        return Some(ErlExpr::Let {
            bindings: vec![ErlLetBinding {
                pattern: ErlPattern::Var(updated_receiver.clone()),
                value: lower_syntax_native_vector_receiver_call(
                    &receiver_type,
                    method,
                    left,
                    args,
                    ctx,
                    env,
                )?,
            }],
            body: Box::new(ErlExpr::Var(updated_receiver)),
        });
    }
    if let Some(value) = lower_http_response_receiver_method_call(
        &receiver_type,
        method,
        left,
        args,
        &right.arg_names,
        ctx,
        env,
    ) {
        let updated_receiver = "_TerlanMutReceiver".to_string();
        return Some(ErlExpr::Let {
            bindings: vec![ErlLetBinding {
                pattern: ErlPattern::Var(updated_receiver.clone()),
                value,
            }],
            body: Box::new(ErlExpr::Var(updated_receiver)),
        });
    }

    let receiver_target = ctx.receiver_method_target(&receiver_type, method, arity)?;
    if !receiver_target.mutable {
        return None;
    }

    let mut lowered_args = Vec::with_capacity(receiver_target.fixed_arity + 1);
    lowered_args.push(lower_syntax_expr_with_env(left, ctx, env)?);
    lowered_args.extend(lower_syntax_defaulted_receiver_method_args(
        args,
        &right.arg_names,
        receiver_target,
        ctx,
        env,
    )?);

    let updated_receiver = "_TerlanMutReceiver".to_string();
    Some(ErlExpr::Let {
        bindings: vec![ErlLetBinding {
            pattern: ErlPattern::Var(updated_receiver.clone()),
            value: ErlExpr::Call {
                module: None,
                function: method.to_string(),
                args: lowered_args,
            },
        }],
        body: Box::new(ErlExpr::Var(updated_receiver)),
    })
}

/// Infers the receiver type that should flow through a pipe chain.
///
/// Inputs:
/// - `expr`: source expression used as a pipe receiver.
/// - `env`: lexical lowering environment containing known value types.
///
/// Output:
/// - Normalized receiver type text when the receiver can be inferred.
/// - `None` when the expression shape has no receiver-type evidence.
///
/// Transformation:
/// - Reads ordinary expression receiver types through the existing trait
///   dispatch inference helper. For nested pipe expressions, follows the left
///   side of the pipe because mutable receiver pipe lowering preserves the
///   original receiver type across each mutating step.
fn infer_syntax_pipe_receiver_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    if matches!(expr.kind, SyntaxExprKind::BinaryOp) && expr.operator.as_deref() == Some("|>") {
        return infer_syntax_pipe_receiver_type(expr.children.first()?, ctx, env);
    }

    infer_syntax_trait_dispatch_type(expr, ctx, env)
}
