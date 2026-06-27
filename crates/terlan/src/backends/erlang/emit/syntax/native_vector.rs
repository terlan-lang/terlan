use super::*;

/// Erlang module name for the native vector SafeNative bridge.
const NATIVE_VECTOR_BRIDGE_MODULE: &str = "std_native_collections_vector_safe_native";

/// Lowers an explicit native vector module call to the bridge module.
///
/// Inputs:
/// - `module`: source, alias-resolved, or Erlang-normalized module name.
/// - `function`: source-visible Vector function.
/// - `args`: source argument expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` when the call targets a supported native Vector function.
/// - `None` when the module/function pair is unrelated to native Vector.
///
/// Transformation:
/// - Rewrites `std.native.collections.Vector` calls to the mandatory BEAM
///   bridge module so generated user modules never call an unresolved
///   `std_native_collections_vector` helper or lower Vector to BEAM lists.
pub(super) fn lower_syntax_native_vector_module_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let function = native_vector_bridge_function(function)?;
    if !is_native_vector_module(module) {
        return None;
    }
    native_vector_bridge_call(function, args, ctx, env)
}

/// Lowers a native vector receiver method to the bridge module.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type text.
/// - `method`: receiver method name.
/// - `receiver`: source receiver expression.
/// - `args`: source non-receiver argument expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` when the receiver is a Vector and method is bridge-owned.
/// - `None` when the receiver/method pair is not native Vector.
///
/// Transformation:
/// - Prepends the receiver expression to the bridge call arguments, preserving
///   the receiver-first method convention while routing execution through the
///   opaque handle bridge.
pub(super) fn lower_syntax_native_vector_receiver_call(
    receiver_type: &str,
    method: &str,
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !is_native_vector_receiver_type(receiver_type) {
        return None;
    }
    let function = native_vector_bridge_function(method)?;
    let mut bridge_args = Vec::with_capacity(args.len() + 1);
    bridge_args.push(receiver.clone());
    bridge_args.extend(args.iter().cloned());
    native_vector_bridge_call(function, &bridge_args, ctx, env)
}

/// Returns whether a receiver method mutates a native vector handle.
///
/// Inputs:
/// - `receiver_type`: inferred receiver type text.
/// - `method`: receiver method name.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - `true` for mutable native Vector receiver methods.
/// - `false` for observers, unrelated receiver types, and unsupported arities.
///
/// Transformation:
/// - Gives sequence lowering the same closed method set as direct bridge call
///   lowering so command-style receiver rebinding cannot drift from the bridge
///   ABI.
pub(super) fn is_native_vector_mutating_receiver_method(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> bool {
    is_native_vector_receiver_type(receiver_type)
        && matches!(
            (method, arg_count),
            ("set_at", 2) | ("swap", 2) | ("push", 1)
        )
}

/// Returns whether a type text denotes the native Vector type.
///
/// Inputs:
/// - `receiver_type`: inferred source type text.
///
/// Output:
/// - `true` when the nominal type head is `Vector`.
///
/// Transformation:
/// - Uses the existing nominal type-head extraction because imported summary
///   type text may be generic or qualified before lowering.
pub(super) fn is_native_vector_receiver_type(receiver_type: &str) -> bool {
    receiver_type_head(receiver_type) == "Vector"
}

/// Lowers already normalized native vector bridge arguments.
///
/// Inputs:
/// - `function`: bridge function name.
/// - `args`: source expressions in bridge argument order.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang call into `std_native_collections_vector_safe_native`.
///
/// Transformation:
/// - Recursively lowers arguments and applies the fixed bridge module name.
fn native_vector_bridge_call(
    function: &str,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    Some(ErlExpr::Call {
        module: Some(NATIVE_VECTOR_BRIDGE_MODULE.to_string()),
        function: function.to_string(),
        args: args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Returns whether a module name denotes the native vector source API.
///
/// Inputs:
/// - `module`: source or Erlang-normalized module name.
///
/// Output:
/// - `true` for canonical, normalized, and imported short vector module names.
///
/// Transformation:
/// - Accepts the names visible at different call-routing stages without
///   broadening the bridge to arbitrary native modules.
fn is_native_vector_module(module: &str) -> bool {
    matches!(
        module,
        "std.native.collections.Vector" | "std_native_collections_vector" | "Vector"
    )
}

/// Maps a source Vector function to a bridge function.
///
/// Inputs:
/// - `function`: source-visible function name.
///
/// Output:
/// - Bridge function name for supported vector operations.
/// - `None` for unrelated calls.
///
/// Transformation:
/// - Keeps the native vector bridge surface closed and explicit.
fn native_vector_bridge_function(function: &str) -> Option<&'static str> {
    match function {
        "new" => Some("new"),
        "from_list" => Some("from_list"),
        "length" | "len" => Some("length"),
        "get_at" => Some("get_at"),
        "set_at" => Some("set_at"),
        "swap" => Some("swap"),
        "push" => Some("push"),
        "to_list" => Some("to_list"),
        _ => None,
    }
}
