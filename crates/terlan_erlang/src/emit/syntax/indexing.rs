use super::*;

/// Lowers bracket index reads through the `IndexGet.get_at` trait path.
///
/// Inputs:
/// - `expr`: syntax-output index expression with collection and index children.
/// - `ctx`: syntax lowering context containing trait wrappers and dispatch
///   metadata.
/// - `env`: lexical environment with inferred local value types.
///
/// Output:
/// - `Some(ErlExpr)` using trait-wrapper dispatch when `IndexGet` metadata is
///   visible for the collection type.
/// - `Some(ErlExpr::Index)` fallback for older fixed-array/raw index lowering
///   until every bracket read is fully target-neutral.
/// - `None` when either operand cannot lower.
///
/// Transformation:
/// - Rewrites `collection[index]` to the same backend ABI used by
///   `IndexGet.get_at(collection, index)`: hidden generic-bound dictionary
///   dispatch first, then typed implementation wrapper dispatch, then generic
///   trait dictionary wrapper dispatch. This keeps bracket syntax aligned with
///   traits while preserving the previous raw-index behavior as a temporary
///   compatibility fallback for shapes not yet covered by `IndexGet`.
pub(super) fn lower_syntax_index_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if expr.children.len() != 2 {
        return None;
    }

    let collection = expr.children.first()?;
    let index = expr.children.get(1)?;
    let args = vec![collection.clone(), index.clone()];

    if let Some(expr) = lower_syntax_bound_trait_method_call("IndexGet", "get_at", &args, ctx, env)
    {
        return Some(expr);
    }

    if let Some(collection_type) = infer_syntax_trait_dispatch_type(collection, env) {
        if let Some(wrapper) =
            ctx.typed_trait_method_wrapper("IndexGet", "get_at", &collection_type)
        {
            let mut lowered_args = Vec::with_capacity(args.len() + 1);
            lowered_args.push(trait_dictionary_expr("IndexGet", "get_at"));
            lowered_args.extend(
                args.iter()
                    .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                    .collect::<Option<Vec<_>>>()?,
            );
            return Some(ErlExpr::Call {
                module: None,
                function: wrapper.clone(),
                args: lowered_args,
            });
        }
    }

    if let Some(wrapper) = ctx.trait_method_wrapper("IndexGet", "get_at") {
        let mut lowered_args = Vec::with_capacity(args.len() + 1);
        lowered_args.push(trait_dictionary_expr("IndexGet", "get_at"));
        lowered_args.extend(
            args.iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        );
        return Some(ErlExpr::Call {
            module: None,
            function: wrapper.clone(),
            args: lowered_args,
        });
    }

    Some(ErlExpr::Index {
        value: Box::new(lower_syntax_expr_with_env(collection, ctx, env)?),
        index: Box::new(lower_syntax_expr_with_env(index, ctx, env)?),
    })
}

/// Lowers indexed assignment as an expression.
///
/// Inputs:
/// - `expr`: syntax-output `IndexAssign` expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` that evaluates the backend update and returns `unit`.
/// - `None` when the assignment shape or selected update call cannot lower.
///
/// Transformation:
/// - Uses the same update call as sequence rebinding, but discards the updated
///   receiver value for direct expression use because source-level indexed
///   assignment has `Unit` type.
pub(super) fn lower_syntax_index_assign_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let value = lower_syntax_index_assign_update_call(expr, ctx, env)?;
    Some(ErlExpr::Let {
        bindings: vec![ErlLetBinding {
            name: "_TerlanIndexAssignIgnored".to_string(),
            value,
        }],
        body: Box::new(ErlExpr::Atom("unit".to_string())),
    })
}

/// Lowers indexed assignment to a backend update call.
///
/// Inputs:
/// - `expr`: syntax-output `IndexAssign` expression with collection, index, and
///   value children.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` for the backend call that produces the updated collection.
/// - `None` when no mutable receiver method or trait-wrapper path is visible.
///
/// Transformation:
/// - Prefers the canonical mutable receiver method shape
///   `(mut collection: C) set_at(index, value): Unit`, then falls back to
///   `IndexSet.set_at` trait-wrapper dispatch for implementation shapes that
///   expose wrapper metadata.
pub(super) fn lower_syntax_index_assign_update_call(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(expr.kind, SyntaxExprKind::IndexAssign) || expr.children.len() != 3 {
        return None;
    }

    let collection = expr.children.first()?;
    let index = expr.children.get(1)?;
    let value = expr.children.get(2)?;
    let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;

    if ctx
        .receiver_method_target(&collection_type, "set_at", 2)
        .is_some_and(|target| target.mutable)
    {
        return Some(ErlExpr::Call {
            module: None,
            function: "set_at".to_string(),
            args: vec![
                lower_syntax_expr_with_env(collection, ctx, env)?,
                lower_syntax_expr_with_env(index, ctx, env)?,
                lower_syntax_expr_with_env(value, ctx, env)?,
            ],
        });
    }

    let args = vec![collection.clone(), index.clone(), value.clone()];
    if let Some(expr) = lower_syntax_bound_trait_method_call("IndexSet", "set_at", &args, ctx, env)
    {
        return Some(expr);
    }

    if let Some(wrapper) = ctx.typed_trait_method_wrapper("IndexSet", "set_at", &collection_type) {
        let mut lowered_args = Vec::with_capacity(args.len() + 1);
        lowered_args.push(trait_dictionary_expr("IndexSet", "set_at"));
        lowered_args.extend(
            args.iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        );
        return Some(ErlExpr::Call {
            module: None,
            function: wrapper.clone(),
            args: lowered_args,
        });
    }

    if let Some(wrapper) = ctx.trait_method_wrapper("IndexSet", "set_at") {
        let mut lowered_args = Vec::with_capacity(args.len() + 1);
        lowered_args.push(trait_dictionary_expr("IndexSet", "set_at"));
        lowered_args.extend(
            args.iter()
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        );
        return Some(ErlExpr::Call {
            module: None,
            function: wrapper.clone(),
            args: lowered_args,
        });
    }

    None
}
