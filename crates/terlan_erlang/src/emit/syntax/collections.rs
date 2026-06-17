use super::*;

/// Lowers selected std collection trait calls through collection bridges.
///
/// Inputs:
/// - `module_name`: provider module that owns the imported trait.
/// - `trait_name`: source trait name from the provider module.
/// - `method`: trait method name being called.
/// - `args`: source-visible call arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression for selected collection trait bridges.
/// - `None` when the trait call is not a closed std collection bridge.
///
/// Transformation:
/// - Reuses iterator-list traversal bridges for std trait syntax so
///   `Enumerable.each(values, cb)`, `Enumerable.map(values, cb)`,
///   `Enumerable.filter(values, predicate)`, and
///   `Enumerable.fold(values, initial, reducer)` preserve the same backend
///   behavior for the selected `List[T]`, `Map[K, V]`, and `Set[T]`
///   conformances.
pub(super) fn lower_syntax_std_collection_trait_bridge(
    module_name: &str,
    trait_name: &str,
    method: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match (module_name, trait_name, method, args) {
        ("std.collections.Enumerable", "Enumerable", "each", [collection, callback]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if !is_supported_enumerable_receiver(&collection_type) {
                return None;
            }
            lower_syntax_list_each_bridge(collection, callback, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "map", [collection, callback]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if !is_supported_enumerable_receiver(&collection_type) {
                return None;
            }
            lower_syntax_list_map_bridge(collection, callback, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "filter", [collection, predicate]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if !is_supported_enumerable_receiver(&collection_type) {
                return None;
            }
            lower_syntax_list_filter_bridge(collection, predicate, ctx, env)
        }
        ("std.collections.Enumerable", "Enumerable", "fold", [collection, initial, reducer]) => {
            let collection_type = infer_syntax_trait_dispatch_type(collection, env)?;
            if !is_supported_enumerable_receiver(&collection_type) {
                return None;
            }
            lower_syntax_list_fold_bridge(collection, initial, reducer, ctx, env)
        }
        _ => None,
    }
}

/// Lowers the release `List.each(cb)` receiver traversal consumer.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: expected single callback expression after the receiver.
/// - `ctx`: syntax lowering context for lowering callback and receiver
///   expressions.
/// - `env`: local type environment used to prove the receiver is a `List`.
///
/// Output:
/// - `Some(ErlExpr)` for `list.each(cb)` when the receiver type resolves to
///   `List[...]`.
/// - `None` for non-field callees, other methods, wrong arity, or non-list
///   receivers.
///
/// Transformation:
/// - Implements the source contract `Iterator.each(list.iterator(), cb)` on
///   the BEAM backend by applying the callback over the immutable list with
///   `lists:foreach/2`, then normalizing the result to Terlan `Unit`.
pub(super) fn lower_syntax_list_each_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    if callee.text.as_deref()? != "each" || args.len() != 1 {
        return None;
    }
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, env)?;
    if receiver_type_head(&receiver_type) != "List" {
        return None;
    }
    lower_syntax_list_each_bridge(receiver, &args[0], ctx, env)
}

/// Returns whether an inferred receiver type has a closed Enumerable bridge.
///
/// Inputs:
/// - `receiver_type`: normalized source type inferred for the collection value.
///
/// Output:
/// - `true` for the std collection heads admitted into the executable
///   Enumerable bridge.
/// - `false` for user collections or unsupported std collections.
///
/// Transformation:
/// - Extracts the nominal type head and admits only List, Map, and Set so trait
///   calls cannot accidentally bypass ordinary method generation.
fn is_supported_enumerable_receiver(receiver_type: &str) -> bool {
    matches!(
        receiver_type_head(receiver_type).as_str(),
        "List" | "Map" | "Set"
    )
}

/// Lowers a collection expression to the selected iterator-list representation.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to a List, Map, or Set.
/// - `env`: local type environment used to infer the collection head.
/// - `ctx`: syntax lowering context for lowering the receiver expression.
///
/// Output:
/// - Erlang list expression used by BEAM list traversal helpers.
/// - `None` when the receiver cannot be lowered or is not a supported
///   Enumerable collection.
///
/// Transformation:
/// - Leaves List values unchanged, converts Map values to `{Key, Value}` pairs
///   with `maps:to_list/1`, and converts Set values to key lists with
///   `maps:keys/1`.
fn lower_syntax_enumerable_source(
    receiver: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_expr_with_env(receiver, ctx, env)?;
    match receiver_type_head(&infer_syntax_trait_dispatch_type(receiver, env)?).as_str() {
        "List" => Some(lowered_receiver),
        "Map" => Some(ErlExpr::Call {
            module: Some("maps".to_string()),
            function: "to_list".to_string(),
            args: vec![lowered_receiver],
        }),
        "Set" => Some(ErlExpr::Call {
            module: Some("maps".to_string()),
            function: "keys".to_string(),
            args: vec![lowered_receiver],
        }),
        _ => None,
    }
}

/// Lowers a collection plus callback to the selected BEAM foreach bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the collection being
///   traversed.
/// - `callback`: source expression that evaluates to a function value.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that applies the callback to each list value and returns
///   Terlan `Unit`.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Converts supported collections into the backend iterator-list shape,
///   emits `lists:foreach/2`, then wraps the target result as `unit` so
///   trait-facing `each` preserves Terlan's command-style return contract.
fn lower_syntax_list_each_bridge(
    receiver: &SyntaxExprOutput,
    callback: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_enumerable_source(receiver, ctx, env)?;
    let lowered_callback = lower_syntax_expr_with_env(callback, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "begin lists:foreach({}, {}), unit end",
        lowered_callback.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a collection plus callback to the selected BEAM map bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the collection being
///   transformed.
/// - `callback`: source expression that evaluates to a function value.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that applies the callback to each list value and returns
///   the transformed list.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Converts supported collections into the backend iterator-list shape, then
///   emits `lists:map/2` with the lowered callback and receiver so
///   `Enumerable.map(values, cb)` has a closed backend bridge while the source
///   contract remains trait-shaped and target-neutral.
fn lower_syntax_list_map_bridge(
    receiver: &SyntaxExprOutput,
    callback: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_enumerable_source(receiver, ctx, env)?;
    let lowered_callback = lower_syntax_expr_with_env(callback, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:map({}, {})",
        lowered_callback.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a collection plus predicate callback to the selected BEAM filter bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the collection being
///   filtered.
/// - `predicate`: source expression that evaluates to a boolean callback.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that keeps each list value for which the predicate
///   returns true.
/// - `None` when either expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Converts supported collections into the backend iterator-list shape, then
///   emits `lists:filter/2` with the lowered predicate and receiver so
///   `Enumerable.filter(values, predicate)` has a closed backend bridge while
///   the source contract remains trait-shaped and target-neutral.
fn lower_syntax_list_filter_bridge(
    receiver: &SyntaxExprOutput,
    predicate: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_enumerable_source(receiver, ctx, env)?;
    let lowered_predicate = lower_syntax_expr_with_env(predicate, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:filter({}, {})",
        lowered_predicate.render(),
        lowered_receiver.render()
    )))
}

/// Lowers a collection, initial accumulator, and reducer to the BEAM fold bridge.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the collection being
///   folded.
/// - `initial`: source expression that evaluates to the initial accumulator.
/// - `reducer`: source expression that evaluates to `(U, T) -> U`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression that folds values from left to right and returns the
///   final accumulator.
/// - `None` when any expression cannot be lowered by the syntax emitter.
///
/// Transformation:
/// - Converts supported collections into the backend iterator-list shape, then
///   emits `lists:foldl/3` while adapting Erlang's `(Value, Acc)` callback
///   convention to Terlan's accumulator-first reducer shape `(Acc, Value)`.
fn lower_syntax_list_fold_bridge(
    receiver: &SyntaxExprOutput,
    initial: &SyntaxExprOutput,
    reducer: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_receiver = lower_syntax_enumerable_source(receiver, ctx, env)?;
    let lowered_initial = lower_syntax_expr_with_env(initial, ctx, env)?;
    let lowered_reducer = lower_syntax_expr_with_env(reducer, ctx, env)?;
    Some(ErlExpr::Raw(format!(
        "lists:foldl(fun(TerlanFoldValue, TerlanFoldAcc) -> ({reducer})(TerlanFoldAcc, TerlanFoldValue) end, {initial}, {receiver})",
        reducer = lowered_reducer.render(),
        initial = lowered_initial.render(),
        receiver = lowered_receiver.render(),
    )))
}
