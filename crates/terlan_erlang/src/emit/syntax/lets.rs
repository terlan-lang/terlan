use super::*;

/// Lowers a syntax-output let expression to an Erlang scoped sequence.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding-name patterns and value
///   children.
/// - `ctx`, `env`: active syntax lowering context and lexical field/type
///   environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when the let shape and all values lower.
/// - `None` for malformed let output or unsupported child expressions.
///
/// Transformation:
/// - Converts each Terlan binding to an Erlang single-assignment variable
///   match, extending the local lowering environment with each simple binding
///   so later bindings and the final body can resolve receiver-method calls on
///   local values. When no explicit body child exists, the expression result is
///   the final binding variable.
pub(super) fn lower_syntax_let_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if expr.patterns.is_empty()
        || expr.children.len() < expr.patterns.len()
        || expr.children.len() > expr.patterns.len() + 1
    {
        return None;
    }

    let mut let_env = env.clone();
    let bindings = expr
        .patterns
        .iter()
        .zip(expr.children.iter())
        .map(|(pattern, value)| {
            let name = pattern.text.as_deref()?;
            let lowered_value = lower_syntax_expr_with_env(value, ctx, &let_env)?;
            if let Some(value_type) = infer_syntax_trait_dispatch_type(value, &let_env)
                .or_else(|| infer_syntax_collection_constructor_type(value, ctx, &let_env))
            {
                let_env.value_types.insert(name.to_string(), value_type);
            }
            let_env.value_locals.insert(name.to_string());
            Some(ErlLetBinding {
                name: sanitize_erlang_var(name),
                value: lowered_value,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = match expr.children.get(expr.patterns.len()) {
        Some(body) => lower_syntax_expr_with_env(body, ctx, &let_env)?,
        None => ErlExpr::Var(bindings.last()?.name.clone()),
    };

    Some(ErlExpr::Let {
        bindings,
        body: Box::new(body),
    })
}

/// Infers the receiver type produced by selected primitive constructor calls.
///
/// Inputs:
/// - `expr`: syntax-output expression assigned to a local binding.
/// - `ctx`: syntax lowering context used to resolve module aliases.
/// - `env`: current local lowering environment used to distinguish
///   method-shaped module calls from receiver calls.
///
/// Output:
/// - `Some("List")`, `Some("Map")`, `Some("Set")`, or `Some("Task")` for
///   recognized primitive constructors.
/// - `None` for non-call expressions, non-primitive constructors, or calls
///   that cannot be resolved through the primitive registry.
///
/// Transformation:
/// - Recognizes both explicit remote calls and method-shaped module calls,
///   maps them through the same primitive registry used for lowering, and
///   returns only the nominal receiver head needed by later receiver-method
///   dispatch.
fn infer_syntax_collection_constructor_type(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    if !matches!(expr.kind, SyntaxExprKind::Call) {
        return None;
    }
    let callee = expr.children.first()?;
    let args = &expr.children[1..];
    let (module, function) = match expr.remote.as_deref() {
        Some(remote) => (
            ctx.resolve_remote_module(remote),
            syntax_expr_name(callee)?.to_string(),
        ),
        None => syntax_method_shaped_remote_call_parts(callee, ctx, env)?,
    };
    match primitive_function_intrinsic(&module, &function, args.len())? {
        CorePrimitiveIntrinsic::ListNew => Some("List".to_string()),
        CorePrimitiveIntrinsic::MapNew => Some("Map".to_string()),
        CorePrimitiveIntrinsic::SetNew => Some("Set".to_string()),
        CorePrimitiveIntrinsic::TaskDone => Some("Task".to_string()),
        CorePrimitiveIntrinsic::BeamAgentStart => Some("Agent".to_string()),
        CorePrimitiveIntrinsic::BeamGenServerStart => Some("ServerRef".to_string()),
        CorePrimitiveIntrinsic::BeamNativeBridgeStart => Some("NativeBridge".to_string()),
        CorePrimitiveIntrinsic::BeamSupervisorChildSpec => Some("ChildSpec".to_string()),
        _ => None,
    }
}
