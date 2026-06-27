use super::*;

/// Lowers `std.http.Request.Request` receiver accessors to request-map reads.
///
/// Inputs:
/// - `receiver_type`: inferred source type for the method receiver.
/// - `method`: receiver method name.
/// - `receiver`: source expression that evaluates to the BEAM request map.
/// - `args`, `arg_names`: method arguments and optional source argument names.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression for supported request-map accessors.
/// - `None` when the receiver is not a request or the method is unsupported.
///
/// Transformation:
/// - Keeps web handler source code target-neutral while the current BEAM
///   handler bridge represents requests as Erlang maps. Required accessors read
///   top-level map keys directly. Optional accessors normalize Terlan `String`
///   keys to binaries, search nested maps, and return `{'some', Value}` or
///   `'none'`.
pub(super) fn lower_http_request_receiver_method_call(
    receiver_type: &str,
    method: &str,
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !is_http_request_receiver_type(receiver_type) {
        return None;
    }
    match (method, args.len()) {
        ("method", 0) => lower_http_request_required_field(receiver, "method", ctx, env),
        ("path", 0) => lower_http_request_required_field(receiver, "path", ctx, env),
        ("body_text", 0) => lower_http_request_required_field(receiver, "body", ctx, env),
        ("body_json", 0) => lower_http_request_body_json(receiver, ctx, env),
        ("query_string", 0) => {
            lower_http_request_required_field(receiver, "query_string", ctx, env)
        }
        ("param", 1) => {
            lower_http_request_optional_map_field(receiver, "params", args, arg_names, ctx, env)
        }
        ("query", 1) => {
            lower_http_request_optional_map_field(receiver, "query", args, arg_names, ctx, env)
        }
        ("header", 1) => {
            lower_http_request_optional_map_field(receiver, "headers", args, arg_names, ctx, env)
        }
        ("cookie", 1) => {
            lower_http_request_optional_map_field(receiver, "cookies", args, arg_names, ctx, env)
        }
        _ => None,
    }
}

/// Tests whether a receiver type names the standard HTTP request type.
///
/// Inputs:
/// - `receiver_type`: normalized receiver type text from syntax lowering.
///
/// Output:
/// - `true` for `std.http.Request.Request` or its imported short name.
///
/// Transformation:
/// - Accepts the qualified std type and the short `Request` type used after
///   `import type std.http.Request.Request.`. This mirrors the current
///   syntax-bridge type environment, which does not retain import provenance
///   for local parameter annotations.
fn is_http_request_receiver_type(receiver_type: &str) -> bool {
    receiver_type_has_head(receiver_type, "std.http.Request.Request")
        || receiver_type_head(receiver_type) == "Request"
}

/// Lowers `std.http.Request.body_json()` for direct BEAM-backed handlers.
///
/// Inputs:
/// - `receiver`: source request expression.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `Result[Json, Error]` expression backed by OTP's `json:decode/1`.
///
/// Transformation:
/// - Reads the buffered request body from the served-handler request map,
///   decodes it through OTP's maintained JSON parser, wraps successful values
///   as `{ok, Value}`, and maps parser failures into the standard `Error`
///   record shape.
fn lower_http_request_body_json(
    receiver: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let request = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    let error_record = map_struct_name("Error");
    Some(ErlExpr::Raw(format!(
        "try json:decode(maps:get(body, {request})) of Value -> {{ok, Value}} catch _:_ -> {{error, #{error_record}{{code = invalid_json, message = \"invalid JSON request body\"}}}} end"
    )))
}

/// Lowers a required top-level HTTP request map field.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the request map.
/// - `field`: Erlang atom key stored in the request map.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - `maps:get(Field, Request)` as an Erlang expression.
///
/// Transformation:
/// - Reuses ordinary receiver lowering, then emits a backend-owned map lookup
///   for fields that are always present in handler request maps.
fn lower_http_request_required_field(
    receiver: &SyntaxExprOutput,
    field: &str,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let request = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    Some(ErlExpr::Raw(format!("maps:get({field}, {request})")))
}

/// Lowers an optional nested HTTP request map lookup.
///
/// Inputs:
/// - `receiver`: source expression that evaluates to the request map.
/// - `field`: top-level request map key for the nested lookup map.
/// - `args`, `arg_names`: method key argument and optional source name.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `case maps:find(...)` expression returning Terlan `Option`.
///
/// Transformation:
/// - Orders named `name = ...` calls, lowers the key expression, converts the
///   key to a binary to match the handler bridge map representation, and wraps
///   found values in `{'some', Value}`.
fn lower_http_request_optional_map_field(
    receiver: &SyntaxExprOutput,
    field: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let ordered_args = ordered_primitive_receiver_method_args(field, args, arg_names)?;
    let key = lower_syntax_expr_with_env(ordered_args.first()?, ctx, env)?.render();
    let request = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "case maps:find(unicode:characters_to_binary({key}), maps:get({field}, {request}, #{{}})) of {{ok, Value}} -> {{'some', Value}}; error -> 'none' end"
    )))
}
