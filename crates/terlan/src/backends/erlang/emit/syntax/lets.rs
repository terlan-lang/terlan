use super::*;

/// Lowers a syntax-output let expression to an Erlang scoped sequence.
///
/// Inputs:
/// - `expr`: syntax-output let node with binding patterns and value children.
/// - `ctx`, `env`: active syntax lowering context and lexical field/type
///   environment.
///
/// Output:
/// - `Some(ErlExpr::Let)` when the let shape and all values lower.
/// - `None` for malformed let output or unsupported child expressions.
///
/// Transformation:
/// - Converts each Terlan binding to an Erlang single-assignment pattern match,
///   extending the local lowering environment for simple variable bindings so
///   later bindings and the final body can resolve receiver-method calls on
///   local values. When no explicit body child exists, the expression result is
///   the final simple binding variable.
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
            let lowered_pattern = lower_syntax_pattern(pattern, ctx)?;
            let lowered_value = lower_syntax_expr_with_env(value, ctx, &let_env)?;
            if matches!(pattern.kind, SyntaxPatternKind::Var) {
                let name = pattern.text.as_deref()?;
                if let Some(value_type) = infer_syntax_trait_dispatch_type(value, ctx, &let_env)
                    .or_else(|| infer_syntax_collection_constructor_type(value, ctx, &let_env))
                {
                    let_env.value_types.insert(name.to_string(), value_type);
                }
            }
            if let Some(value_type) = infer_syntax_trait_dispatch_type(value, ctx, &let_env)
                .or_else(|| infer_syntax_collection_constructor_type(value, ctx, &let_env))
            {
                collect_syntax_pattern_value_types(
                    pattern,
                    &value_type,
                    ctx,
                    &mut let_env.value_types,
                );
            }
            collect_syntax_pattern_value_locals(pattern, &mut let_env.value_locals);
            Some(ErlLetBinding {
                pattern: lowered_pattern,
                value: lowered_value,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let body = match expr.children.get(expr.patterns.len()) {
        Some(body) => lower_syntax_expr_with_env(body, ctx, &let_env)?,
        None => match &bindings.last()?.pattern {
            ErlPattern::Var(name) => ErlExpr::Var(name.clone()),
            _ => return None,
        },
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
/// - The nominal return type for selected imported constructors such as
///   `List(...)`.
/// - `None` for non-call expressions, unsupported constructors, or calls that
///   cannot be resolved through either imported constructor metadata or the
///   primitive registry.
///
/// Transformation:
/// - Recognizes imported constructor shorthand first, then explicit remote
///   primitive calls and method-shaped module calls. The result is reduced to
///   the nominal receiver head needed by later receiver-method dispatch.
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
    if expr.remote.is_none() {
        if let Some(callee_name) = syntax_expr_name(callee) {
            if let Some(target) = ctx.imported_constructor_target(callee_name, args.len()) {
                return Some(receiver_type_head(&target.return_type));
            }
        }
    }
    let (module, function) = match expr.remote.as_deref() {
        Some(remote) => (
            ctx.resolve_remote_module(remote),
            syntax_expr_name(callee)?.to_string(),
        ),
        None => syntax_method_shaped_remote_call_parts(callee, ctx, env)?,
    };
    match primitive_function_intrinsic(&module, &function, args.len())? {
        CorePrimitiveIntrinsic::ListNew => Some("List".to_string()),
        CorePrimitiveIntrinsic::MapNew | CorePrimitiveIntrinsic::MapFromEntries => {
            Some("Map".to_string())
        }
        CorePrimitiveIntrinsic::SetNew | CorePrimitiveIntrinsic::SetFromList => {
            Some("Set".to_string())
        }
        CorePrimitiveIntrinsic::TaskDone => Some("Task".to_string()),
        CorePrimitiveIntrinsic::BeamAgentStart => Some("Agent".to_string()),
        CorePrimitiveIntrinsic::BeamGenServerStart => Some("ServerRef".to_string()),
        CorePrimitiveIntrinsic::BeamNativeBridgeStart => Some("NativeBridge".to_string()),
        CorePrimitiveIntrinsic::BeamSupervisorChildSpec => Some("ChildSpec".to_string()),
        _ => None,
    }
}
