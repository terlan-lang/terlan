use super::*;

/// Lowered source expression for a list comprehension.
///
/// Inputs:
/// - Terlan comprehension source expression after `<-`.
///
/// Output:
/// - Native Erlang list source for list-backed comprehensions.
/// - Explicit iterator expression for generic iterable sources.
///
/// Transformation:
/// - Records whether the comprehension can use Erlang's native list
///   comprehension syntax or must lower through an explicit iterator loop.
#[derive(Debug, Clone)]
pub(super) enum LoweredComprehensionSource {
    NativeList(ErlExpr),
    IterableIterator(ErlExpr),
}

/// Classifies and lowers the source side of a list comprehension.
///
/// Inputs:
/// - `source`: comprehension source expression after `<-`.
/// - `ctx`: syntax lowering context containing local receiver-method metadata.
/// - `env`: local value/type environment used to identify non-list iterable
///   sources.
///
/// Output:
/// - `NativeList` for lowered native list inputs.
/// - `IterableIterator` for lowered `iterator(Source)` calls over non-list
///   locals whose type declares a zero-argument receiver `iterator` method.
///
/// Transformation:
/// - Keeps existing list-backed Erlang comprehension lowering for list sources
///   while desugaring the first generic `Iterable` source shape into the
///   compiler-visible iterator-producing receiver method.
pub(super) fn lower_syntax_list_comprehension_source(
    source: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<LoweredComprehensionSource> {
    let Some(receiver_type) = infer_syntax_trait_dispatch_type(source, ctx, env) else {
        return lower_syntax_expr_with_env(source, ctx, env)
            .map(LoweredComprehensionSource::NativeList);
    };

    if receiver_type_head(&receiver_type) == "List" {
        return lower_syntax_expr_with_env(source, ctx, env)
            .map(LoweredComprehensionSource::NativeList);
    }

    if ctx
        .receiver_method_target(&receiver_type, "iterator", 0)
        .is_some()
    {
        return Some(LoweredComprehensionSource::IterableIterator(
            ErlExpr::Call {
                module: None,
                function: "iterator".to_string(),
                args: vec![lower_syntax_expr_with_env(source, ctx, env)?],
            },
        ));
    }

    lower_syntax_expr_with_env(source, ctx, env).map(LoweredComprehensionSource::NativeList)
}

/// Lowers a list-comprehension expression.
///
/// Inputs:
/// - `expr`: syntax-output list comprehension with yield, source, pattern, and
///   optional guard.
/// - `ctx`, `env`: syntax lowering context and local type environment.
///
/// Output:
/// - Native Erlang list-comprehension expression for native list sources.
/// - Explicit state-passing loop for generic iterable sources.
///
/// Transformation:
/// - Preserves the existing backend-native lowering for list-backed
///   comprehensions and rewrites iterable sources to `iterator(Source)`,
///   repeated next-step matching, ordered guard evaluation, and reverse-order
///   accumulation.
pub(super) fn lower_syntax_list_comprehension_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let value = expr.children.first()?;
    let source = expr.children.get(1)?;
    let pattern = expr.patterns.first()?;
    let guard = expr.children.get(2);
    let lowered_value = lower_syntax_expr_with_env(value, ctx, env)?;
    let lowered_pattern = lower_syntax_pattern(pattern, ctx)?;
    let lowered_guard = match guard {
        Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
        None => None,
    };

    match lower_syntax_list_comprehension_source(source, ctx, env)? {
        LoweredComprehensionSource::NativeList(source) => Some(ErlExpr::ListComprehension {
            expr: Box::new(lowered_value),
            pattern: lowered_pattern,
            source: Box::new(source),
            guard: lowered_guard.map(Box::new),
        }),
        LoweredComprehensionSource::IterableIterator(iterator) => {
            Some(lower_syntax_iterable_comprehension_loop(
                expr,
                iterator,
                lowered_pattern,
                lowered_guard,
                lowered_value,
            ))
        }
    }
}

/// Lowers a generic iterable comprehension into an explicit BEAM loop.
///
/// Inputs:
/// - `expr`: original comprehension expression used only for deterministic
///   temporary naming.
/// - `iterator`: lowered iterator-state expression.
/// - `pattern`: lowered generator pattern.
/// - `guard`: optional lowered filter guard.
/// - `value`: lowered yielded expression.
///
/// Output:
/// - Raw Erlang expression implementing state-passing traversal.
///
/// Transformation:
/// - Binds the initial iterator, creates a recursive local fun, repeatedly
///   matches the iterator state as `None` or `Some({value, next})`, skips
///   failed patterns/filters, accumulates yielded values in reverse order, and
///   returns `lists:reverse(Acc)`.
fn lower_syntax_iterable_comprehension_loop(
    expr: &SyntaxExprOutput,
    iterator: ErlExpr,
    pattern: ErlPattern,
    guard: Option<ErlExpr>,
    value: ErlExpr,
) -> ErlExpr {
    let suffix = format!("{}_{}", expr.span.start, expr.span.end);
    let iterator_var = format!("TerlanIterator{}", suffix);
    let loop_var = format!("TerlanIterableLoop{}", suffix);
    let iter_var = format!("TerlanIter{}", suffix);
    let acc_var = format!("TerlanAcc{}", suffix);
    let raw_value_var = format!("TerlanRawValue{}", suffix);
    let raw_next_var = format!("TerlanRawNext{}", suffix);
    let next_var = format!("TerlanNext{}", suffix);
    let skipped_var = format!("_TerlanSkipped{}", suffix);

    let guard = guard
        .as_ref()
        .map(|guard| format!(" when {}", guard.render()))
        .unwrap_or_default();
    let body = format!(
        "{loop_var}({next_var}, [{value} | {acc_var}])",
        loop_var = loop_var,
        next_var = next_var,
        value = value.render(),
        acc_var = acc_var
    );

    ErlExpr::Raw(format!(
        "begin\n    {iterator_var} = {iterator},\n    {loop_var} = fun {loop_var}({iter_var}, {acc_var}) ->\n        case (case {iter_var} of\n            [{raw_value_var}|{raw_next_var}] -> {{'some', {{{raw_value_var}, {raw_next_var}}}}};\n            [] -> 'none'\n        end) of\n            'none' -> lists:reverse({acc_var});\n            {{'some', {{{pattern}, {next_var}}}}}{guard} -> {body};\n            {{'some', {{{skipped_var}, {next_var}}}}} -> {loop_var}({next_var}, {acc_var})\n        end\n    end,\n    {loop_var}({iterator_var}, [])\nend",
        iterator_var = iterator_var,
        iterator = iterator.render(),
        loop_var = loop_var,
        iter_var = iter_var,
        acc_var = acc_var,
        raw_value_var = raw_value_var,
        raw_next_var = raw_next_var,
        pattern = pattern.render(),
        next_var = next_var,
        guard = guard,
        body = body,
        skipped_var = skipped_var,
    ))
}
