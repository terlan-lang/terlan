use super::http_args::{
    ordered_http_router_group_args, ordered_http_router_handler_args,
    ordered_http_router_receiver_group_args, ordered_http_router_receiver_handler_arg,
    ordered_http_router_receiver_route_args, ordered_http_router_route_args,
};
use super::*;

/// Lowers source-level HTTP router builders to a small BEAM-side route table.
///
/// Inputs:
/// - `module`: resolved source module path for the call.
/// - `function`: router builder function name.
/// - `args`: source router builder arguments.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` for `std.http.Router` builder calls.
/// - `None` for non-router calls or unsupported argument shapes.
///
/// Transformation:
/// - Gives Erlang-emitted modules a concrete representation for router builder
///   expressions while route manifests remain owned by `terlc build`/`serve`.
///   The value is intentionally simple: `{terlan_router, Routes}`.
pub(in crate::backends::erlang::emit::syntax) fn lower_http_router_builder_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if module != "std.http.Router" {
        return None;
    }
    match function {
        "new" if args.is_empty() => Some(ErlExpr::Raw("{terlan_router, []}".to_string())),
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options" => {
            let (router, pattern, handler) = ordered_http_router_route_args(args, arg_names)?;
            lower_http_router_route_append(function, router, pattern, handler, ctx, env)
        }
        "fallback" => {
            let (router, handler) = ordered_http_router_handler_args(args, arg_names, "handler")?;
            lower_http_router_handler_append("fallback", router, None, handler, ctx, env)
        }
        "use" => {
            let (router, middleware) =
                ordered_http_router_handler_args(args, arg_names, "middleware")?;
            lower_http_router_handler_append("use", router, None, middleware, ctx, env)
        }
        "error" => {
            let (router, handler) = ordered_http_router_handler_args(args, arg_names, "handler")?;
            lower_http_router_handler_append("error", router, None, handler, ctx, env)
        }
        "group" => {
            let (router, prefix, configure) = ordered_http_router_group_args(args, arg_names)?;
            lower_http_router_handler_append("group", router, Some(prefix), configure, ctx, env)
        }
        _ => None,
    }
}

/// Lowers router receiver-method calls such as `Router.new().get("/", home)`.
pub(in crate::backends::erlang::emit::syntax) fn lower_http_router_receiver_method_call(
    receiver_type: &str,
    method: &str,
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !is_http_router_receiver_type(receiver_type) {
        return None;
    }
    match method {
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options" => {
            let (pattern, handler) = ordered_http_router_receiver_route_args(args, arg_names)?;
            lower_http_router_route_append(method, receiver, pattern, handler, ctx, env)
        }
        "fallback" => {
            let handler = ordered_http_router_receiver_handler_arg(args, arg_names, "handler")?;
            lower_http_router_handler_append("fallback", receiver, None, handler, ctx, env)
        }
        "use" => {
            let middleware =
                ordered_http_router_receiver_handler_arg(args, arg_names, "middleware")?;
            lower_http_router_handler_append("use", receiver, None, middleware, ctx, env)
        }
        "error" => {
            let handler = ordered_http_router_receiver_handler_arg(args, arg_names, "handler")?;
            lower_http_router_handler_append("error", receiver, None, handler, ctx, env)
        }
        "group" => {
            let (prefix, configure) = ordered_http_router_receiver_group_args(args, arg_names)?;
            lower_http_router_handler_append("group", receiver, Some(prefix), configure, ctx, env)
        }
        _ => None,
    }
}

/// Lowers one HTTP route append operation.
///
/// Inputs:
/// - `method`: HTTP method atom text.
/// - `router`, `pattern`, `handler`: source router expression and route data.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - BEAM expression that appends one route tuple to the router value.
/// - `None` when a child expression cannot be lowered.
///
/// Transformation:
/// - Converts the route pattern to a UTF-8 binary and delegates list append
///   construction to the shared router append helper.
fn lower_http_router_route_append(
    method: &str,
    router: &SyntaxExprOutput,
    pattern: &SyntaxExprOutput,
    handler: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    lower_http_router_append(
        router,
        format!(
            "{{{}, unicode:characters_to_binary({}), {}}}",
            method,
            lower_syntax_expr_with_env(pattern, ctx, env)?.render(),
            lower_syntax_expr_with_env(handler, ctx, env)?.render()
        ),
        ctx,
        env,
    )
}

/// Lowers one HTTP handler-like router append operation.
///
/// Inputs:
/// - `kind`: router entry kind such as `fallback`, `use`, `error`, or `group`.
/// - `router`: source router expression.
/// - `prefix`: optional group prefix expression.
/// - `handler`: handler or configure callback expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - BEAM expression that appends one handler tuple to the router value.
/// - `None` when a child expression cannot be lowered.
///
/// Transformation:
/// - Emits either `{kind, Handler}` or `{kind, Prefix, Handler}` before
///   delegating list append construction to the shared router append helper.
fn lower_http_router_handler_append(
    kind: &str,
    router: &SyntaxExprOutput,
    prefix: Option<&SyntaxExprOutput>,
    handler: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let entry = if let Some(prefix) = prefix {
        format!(
            "{{{}, unicode:characters_to_binary({}), {}}}",
            kind,
            lower_syntax_expr_with_env(prefix, ctx, env)?.render(),
            lower_syntax_expr_with_env(handler, ctx, env)?.render()
        )
    } else {
        format!(
            "{{{}, {}}}",
            kind,
            lower_syntax_expr_with_env(handler, ctx, env)?.render()
        )
    };
    lower_http_router_append(router, entry, ctx, env)
}

/// Appends one lowered entry to the BEAM router tuple.
///
/// Inputs:
/// - `router`: source router expression.
/// - `entry`: already-rendered Erlang route entry.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - BEAM case expression that returns the updated router tuple.
/// - `None` when the router expression cannot be lowered.
///
/// Transformation:
/// - Lowers `{terlan_router, Routes}` into `{terlan_router, Routes ++ [Entry]}`
///   so source receiver chains remain immutable at the BEAM value level.
fn lower_http_router_append(
    router: &SyntaxExprOutput,
    entry: String,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let router = lower_syntax_expr_with_env(router, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "case {router} of {{terlan_router, Routes}} -> {{terlan_router, Routes ++ [{entry}]}} end"
    )))
}

/// Reports whether a receiver type is an HTTP router.
///
/// Inputs:
/// - `receiver_type`: normalized receiver type text from syntax lowering.
///
/// Output:
/// - `true` for `std.http.Router.Router` or its imported short name.
///
/// Transformation:
/// - Mirrors response receiver type detection so imported router values can use
///   short `Router` annotations after a type import.
fn is_http_router_receiver_type(receiver_type: &str) -> bool {
    receiver_type_has_head(receiver_type, "std.http.Router.Router")
        || receiver_type_head(receiver_type) == "Router"
}
