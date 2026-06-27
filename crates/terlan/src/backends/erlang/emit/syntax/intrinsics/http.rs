use super::*;

#[path = "http_args.rs"]
mod http_args;
#[path = "http_request.rs"]
mod http_request;
#[path = "http_router.rs"]
mod http_router;

use http_request::lower_http_request_receiver_method_call;
pub(in crate::backends::erlang::emit::syntax) use http_router::lower_http_router_builder_call;
use http_router::lower_http_router_receiver_method_call;

/// Lowers served-handler HTTP response builders to the temporary BEAM ABI.
///
/// Inputs:
/// - `module`: resolved source module path for the call.
/// - `function`: response builder name.
/// - `args`: source response builder arguments.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - `Some(ErlExpr)` for response builders supported by direct BEAM handlers.
/// - `None` for non-response calls or response builders that require a native
///   resource bridge.
///
/// Transformation:
/// - Maps `std.http.Response.text`, `json_text`, `html`, and `redirect` onto
///   the stable `{terlan_response, ...}` tuple consumed by `terlc serve`.
///   `Response.json(Json)` and file responses stay outside this bridge until
///   JSON handles and file streaming have a canonical direct-BEAM
///   representation.
pub(in crate::backends::erlang::emit::syntax) fn lower_http_response_builder_call(
    module: &str,
    function: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if module != "std.http.Response" {
        return None;
    }
    let body_param = if function == "redirect" {
        "location"
    } else {
        "value"
    };
    let (body, status) = ordered_http_response_builder_args(args, arg_names, body_param, "status")?;
    match (function, args.len()) {
        ("text", 1) => lower_http_response_body_builder(
            body,
            "200",
            "<<\"text/plain; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("text", 2) => lower_http_response_body_builder(
            body,
            &lower_syntax_expr_with_env(status?, ctx, env)?.render(),
            "<<\"text/plain; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("json_text", 1) => lower_http_response_body_builder(
            body,
            "200",
            "<<\"application/json; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("json_text", 2) => lower_http_response_body_builder(
            body,
            &lower_syntax_expr_with_env(status?, ctx, env)?.render(),
            "<<\"application/json; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("html", 1) => lower_http_response_body_builder(
            body,
            "200",
            "<<\"text/html; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("html", 2) => lower_http_response_body_builder(
            body,
            &lower_syntax_expr_with_env(status?, ctx, env)?.render(),
            "<<\"text/html; charset=utf-8\">>",
            ctx,
            env,
        ),
        ("redirect", 1) => lower_http_redirect_response_builder(body, "302", ctx, env),
        ("redirect", 2) => {
            let status = lower_syntax_expr_with_env(status?, ctx, env)?.render();
            lower_http_redirect_response_builder(body, &status, ctx, env)
        }
        _ => None,
    }
}

/// Orders source arguments for supported HTTP response builders.
///
/// Inputs:
/// - `args`: written call arguments.
/// - `arg_names`: optional names parallel to `args`.
/// - `body_param`: first builder parameter name, such as `value` or
///   `location`.
/// - `status_param`: status parameter name.
///
/// Output:
/// - First/body argument plus optional explicit status argument.
///
/// Transformation:
/// - Applies Terlan named-argument rules for the closed response-builder
///   surface without adding a general runtime-capability argument rewriter.
fn ordered_http_response_builder_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    body_param: &str,
    status_param: &str,
) -> Option<(&'a SyntaxExprOutput, Option<&'a SyntaxExprOutput>)> {
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)));
    }
    let mut body = None;
    let mut status = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some(name) if name == body_param => body = Some(arg),
            Some(name) if name == status_param => status = Some(arg),
            Some(_) => return None,
            None if body.is_none() => body = Some(arg),
            None if status.is_none() => status = Some(arg),
            None => return None,
        }
    }
    Some((body?, status))
}

/// Lowers one body-based HTTP response builder.
///
/// Inputs:
/// - `body`: source expression for the response body.
/// - `status`: rendered Erlang status expression.
/// - `content_type`: rendered Erlang binary content type.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang `{terlan_response, Status, ContentType, Body}` expression.
///
/// Transformation:
/// - Lowers the body through the standard syntax bridge and wraps it in the
///   response tuple expected by the current `terlc serve` BEAM runner. The
///   body is normalized to a binary because the runner accepts binary response
///   payloads.
fn lower_http_response_body_builder(
    body: &SyntaxExprOutput,
    status: &str,
    content_type: &str,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let body = lower_syntax_expr_with_env(body, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "{{terlan_response, {status}, {content_type}, unicode:characters_to_binary({body})}}"
    )))
}

/// Lowers an HTTP redirect response builder.
///
/// Inputs:
/// - `location`: source expression for the redirect location.
/// - `status`: rendered Erlang redirect status expression.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang response tuple containing a `Location` header and empty body.
///
/// Transformation:
/// - Preserves the same response shape as the Rust HTTP adapter's redirect
///   helper while targeting the direct BEAM handler bridge. The location is
///   normalized to a binary because the runner validates response headers as
///   binary pairs.
fn lower_http_redirect_response_builder(
    location: &SyntaxExprOutput,
    status: &str,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let location = lower_syntax_expr_with_env(location, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "{{terlan_response, {status}, <<\"text/plain; charset=utf-8\">>, [{{<<\"Location\">>, unicode:characters_to_binary({location})}}], <<>>}}"
    )))
}

/// Lowers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `callee`: field-access callee from a method call expression.
/// - `args`: ordinary call arguments after the receiver.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`: syntax lowering context for module aliases and expression lowering.
/// - `env`: local type environment used to infer receiver primitive type.
///
/// Output:
/// - `Some(ErlExpr::Call)` for known primitive receiver methods.
/// - `None` when the callee is not a primitive method call.
///
/// Transformation:
/// - Rewrites primitive receiver calls such as `"abc".trim()` or
///   `1.to_string()` into CoreIR primitive intrinsic calls and delegates to the
///   shared CoreIR intrinsic Erlang lowerer.
pub(in crate::backends::erlang::emit::syntax) fn lower_syntax_primitive_receiver_method_call(
    callee: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_trait_dispatch_type(receiver, ctx, env)?;
    if let Some(expr) = lower_http_response_receiver_method_call(
        &receiver_type,
        method,
        receiver,
        args,
        arg_names,
        ctx,
        env,
    ) {
        return Some(expr);
    }
    if let Some(expr) = lower_http_request_receiver_method_call(
        &receiver_type,
        method,
        receiver,
        args,
        arg_names,
        ctx,
        env,
    ) {
        return Some(expr);
    }
    if let Some(expr) = lower_http_router_receiver_method_call(
        &receiver_type,
        method,
        receiver,
        args,
        arg_names,
        ctx,
        env,
    ) {
        return Some(expr);
    }
    let intrinsic = primitive_receiver_method_intrinsic(&receiver_type, method, args.len())?;
    let mut lowered_args = Vec::with_capacity(args.len() + 1);
    lowered_args.push(lower_syntax_expr_with_env(receiver, ctx, env)?);
    lowered_args.extend(
        ordered_primitive_receiver_method_args(method, args, arg_names)?
            .into_iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );

    lower_core_primitive_intrinsic_to_erlang(&intrinsic, lowered_args)
}

/// Lowers served-handler HTTP response receiver helpers to response-tuple edits.
///
/// Inputs:
/// - `receiver_type`: inferred source type for the receiver expression.
/// - `method`: response receiver helper name.
/// - `receiver`: source expression that evaluates to the response tuple.
/// - `args`: method arguments.
/// - `arg_names`: optional source argument names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression returning an updated `{terlan_response, ...}` tuple.
/// - `None` when the receiver is not a response or the method is unsupported.
///
/// Transformation:
/// - Lets handler source use `std.http.Response` metadata helpers without
///   depending on a generated backend std module. The helper preserves both
///   current response tuple forms: with and without explicit header lists.
pub(in crate::backends::erlang::emit::syntax) fn lower_http_response_receiver_method_call(
    receiver_type: &str,
    method: &str,
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if !is_http_response_receiver_type(receiver_type) {
        return None;
    }
    match (method, args.len()) {
        ("status", 1) | ("with_status", 1) => {
            let code = ordered_http_response_single_arg(args, arg_names, "code")?;
            lower_http_response_status_update(receiver, code, ctx, env)
        }
        ("header", 2) | ("with_header", 2) => {
            let (name, value) = ordered_http_response_pair_args(args, arg_names, "name", "value")?;
            lower_http_response_header_append(receiver, name, value, ctx, env)
        }
        ("set_cookie_header", 1) => lower_http_response_header_append_literal_name(
            receiver,
            "<<\"Set-Cookie\">>",
            ordered_http_response_single_arg(args, arg_names, "value")?,
            ctx,
            env,
        ),
        ("cookie", 2..=5) | ("with_cookie", 2..=5) => {
            lower_http_response_cookie_helper(receiver, args, arg_names, ctx, env)
        }
        ("cookie_with_options", 2..=10) | ("with_cookie_options", 2..=10) => {
            lower_http_response_cookie_with_options_helper(receiver, args, arg_names, ctx, env)
        }
        ("delete_cookie", 1..=2) | ("with_deleted_cookie", 1..=2) => {
            lower_http_response_delete_cookie_helper(receiver, args, arg_names, ctx, env)
        }
        _ => None,
    }
}

/// Response-cookie helper argument after default completion.
///
/// Inputs:
/// - Source expression references or compiler-known default literal text.
///
/// Output:
/// - A discriminated argument used only by direct HTTP response lowering.
///
/// Transformation:
/// - Lets direct response helpers share one ordering/defaulting path without
///   constructing synthetic syntax-output nodes for default values.
enum HttpResponseHelperArg<'a> {
    /// Argument supplied by source.
    Source(&'a SyntaxExprOutput),
    /// Literal Erlang argument inserted from a source-level default.
    Literal(&'static str),
}

/// Orders and fills defaulted HTTP response helper arguments.
///
/// Inputs:
/// - `args`: written method arguments.
/// - `arg_names`: optional names parallel to `args`.
/// - `params`: declaration parameter names in source order.
/// - `defaults`: default Erlang expressions parallel to `params`.
///
/// Output:
/// - Ordered arguments with defaults inserted for omitted defaulted params.
///
/// Transformation:
/// - Applies the small fixed `std.http.Response` helper parameter contracts
///   used by the direct BEAM bridge. Typechecking owns diagnostics; this
///   helper returns `None` when source metadata does not fit the known helper.
fn ordered_http_response_defaulted_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    params: &[&str],
    defaults: &[Option<&'static str>],
) -> Option<Vec<HttpResponseHelperArg<'a>>> {
    if params.len() != defaults.len() || args.len() > params.len() {
        return None;
    }
    let mut ordered: Vec<Option<&SyntaxExprOutput>> = vec![None; params.len()];
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some(name) => {
                let param_index = params.iter().position(|param| *param == name)?;
                if ordered[param_index].is_some() {
                    return None;
                }
                ordered[param_index] = Some(arg);
            }
            None => {
                if index >= ordered.len() || ordered[index].is_some() {
                    return None;
                }
                ordered[index] = Some(arg);
            }
        }
    }

    ordered
        .into_iter()
        .enumerate()
        .map(|(index, arg)| match arg {
            Some(arg) => Some(HttpResponseHelperArg::Source(arg)),
            None => defaults[index].map(HttpResponseHelperArg::Literal),
        })
        .collect()
}

/// Renders one direct response helper argument as Erlang.
///
/// Inputs:
/// - `arg`: ordered source/default helper argument.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression text for the argument.
///
/// Transformation:
/// - Lowers source expressions through the normal syntax bridge and passes
///   compiler-known default literals through unchanged.
fn render_http_response_helper_arg(
    arg: &HttpResponseHelperArg<'_>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<String> {
    match arg {
        HttpResponseHelperArg::Source(expr) => {
            Some(lower_syntax_expr_with_env(expr, ctx, env)?.render())
        }
        HttpResponseHelperArg::Literal(literal) => Some((*literal).to_string()),
    }
}

/// Lowers `Response.cookie(...)` to a direct `Set-Cookie` response update.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `args`: written helper arguments.
/// - `arg_names`: optional source names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression returning the updated response tuple.
///
/// Transformation:
/// - Mirrors the current `std.http.Cookies.set_header` serialization order for
///   the direct BEAM handler bridge: `name=value; Path=...`, followed by
///   conditional `HttpOnly` and `Secure` attributes.
fn lower_http_response_cookie_helper(
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let ordered = ordered_http_response_defaulted_args(
        args,
        arg_names,
        &["name", "value", "path", "http_only", "secure"],
        &[None, None, Some("\"/\""), Some("false"), Some("false")],
    )?;
    let name = render_http_response_helper_arg(&ordered[0], ctx, env)?;
    let value = render_http_response_helper_arg(&ordered[1], ctx, env)?;
    let path = render_http_response_helper_arg(&ordered[2], ctx, env)?;
    let http_only = render_http_response_helper_arg(&ordered[3], ctx, env)?;
    let secure = render_http_response_helper_arg(&ordered[4], ctx, env)?;
    let header = format!(
        "unicode:characters_to_binary([{name}, \"=\", {value}, \"; Path=\", {path}, case {http_only} of true -> \"; HttpOnly\"; _ -> \"\" end, case {secure} of true -> \"; Secure\"; _ -> \"\" end])"
    );
    lower_http_response_header_append_rendered_value(
        receiver,
        "<<\"Set-Cookie\">>",
        &header,
        ctx,
        env,
    )
}

/// Lowers `Response.cookie_with_options(...)` to a direct cookie header update.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `args`: written helper arguments.
/// - `arg_names`: optional source names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression returning the updated response tuple.
///
/// Transformation:
/// - Mirrors the Rust adapter's deterministic full cookie option order for
///   direct BEAM handlers while keeping validation in the Rust SafeNative path.
fn lower_http_response_cookie_with_options_helper(
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let ordered = ordered_http_response_defaulted_args(
        args,
        arg_names,
        &[
            "name",
            "value",
            "path",
            "domain",
            "max_age",
            "include_max_age",
            "expires",
            "http_only",
            "secure",
            "same_site",
        ],
        &[
            None,
            None,
            Some("\"/\""),
            Some("\"\""),
            Some("0"),
            Some("false"),
            Some("\"\""),
            Some("false"),
            Some("false"),
            Some("\"\""),
        ],
    )?;
    let name = render_http_response_helper_arg(&ordered[0], ctx, env)?;
    let value = render_http_response_helper_arg(&ordered[1], ctx, env)?;
    let path = render_http_response_helper_arg(&ordered[2], ctx, env)?;
    let domain = render_http_response_helper_arg(&ordered[3], ctx, env)?;
    let max_age = render_http_response_helper_arg(&ordered[4], ctx, env)?;
    let include_max_age = render_http_response_helper_arg(&ordered[5], ctx, env)?;
    let expires = render_http_response_helper_arg(&ordered[6], ctx, env)?;
    let http_only = render_http_response_helper_arg(&ordered[7], ctx, env)?;
    let secure = render_http_response_helper_arg(&ordered[8], ctx, env)?;
    let same_site = render_http_response_helper_arg(&ordered[9], ctx, env)?;
    let header = format!(
        "unicode:characters_to_binary([{name}, \"=\", {value}, \"; Path=\", {path}, case {domain} of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Domain=\", {domain}] end, case {include_max_age} of true -> [\"; Max-Age=\", integer_to_list({max_age})]; _ -> \"\" end, case {expires} of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; Expires=\", {expires}] end, case {http_only} of true -> \"; HttpOnly\"; _ -> \"\" end, case {secure} of true -> \"; Secure\"; _ -> \"\" end, case {same_site} of \"\" -> \"\"; <<>> -> \"\"; _ -> [\"; SameSite=\", {same_site}] end])"
    );
    lower_http_response_header_append_rendered_value(
        receiver,
        "<<\"Set-Cookie\">>",
        &header,
        ctx,
        env,
    )
}

/// Lowers `Response.delete_cookie(...)` to an expiring `Set-Cookie` header.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `args`: written helper arguments.
/// - `arg_names`: optional source names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang expression returning the updated response tuple.
///
/// Transformation:
/// - Emits the same deletion shape as `std.http.Cookies.delete_header`:
///   `name=; Path=...; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT`.
fn lower_http_response_delete_cookie_helper(
    receiver: &SyntaxExprOutput,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let ordered = ordered_http_response_defaulted_args(
        args,
        arg_names,
        &["name", "path"],
        &[None, Some("\"/\"")],
    )?;
    let name = render_http_response_helper_arg(&ordered[0], ctx, env)?;
    let path = render_http_response_helper_arg(&ordered[1], ctx, env)?;
    let header = format!(
        "unicode:characters_to_binary([{name}, \"=; Path=\", {path}, \"; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT\"])"
    );
    lower_http_response_header_append_rendered_value(
        receiver,
        "<<\"Set-Cookie\">>",
        &header,
        ctx,
        env,
    )
}

/// Orders a one-argument HTTP response receiver helper.
///
/// Inputs:
/// - `args`: written method arguments.
/// - `arg_names`: optional names parallel to `args`.
/// - `param`: accepted parameter name.
///
/// Output:
/// - Ordered single argument.
///
/// Transformation:
/// - Accepts positional form and `param = value` form for response helpers
///   without broadening general runtime capability argument rewriting.
fn ordered_http_response_single_arg<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    param: &str,
) -> Option<&'a SyntaxExprOutput> {
    if args.len() != 1 {
        return None;
    }
    match arg_names.first().and_then(Option::as_deref) {
        None => args.first(),
        Some(name) if name == param => args.first(),
        Some(_) => None,
    }
}

/// Orders a two-argument HTTP response receiver helper.
///
/// Inputs:
/// - `args`: written method arguments.
/// - `arg_names`: optional names parallel to `args`.
/// - `first_param`, `second_param`: accepted parameter names.
///
/// Output:
/// - Ordered pair of arguments.
///
/// Transformation:
/// - Supports positional and named source calls for response header helpers so
///   `response.with_header(value = "yes", name = "x")` lowers the same as
///   `response.with_header("x", "yes")`.
fn ordered_http_response_pair_args<'a>(
    args: &'a [SyntaxExprOutput],
    arg_names: &'a [Option<String>],
    first_param: &str,
    second_param: &str,
) -> Option<(&'a SyntaxExprOutput, &'a SyntaxExprOutput)> {
    if args.len() != 2 {
        return None;
    }
    if !arg_names.iter().any(Option::is_some) {
        return Some((args.first()?, args.get(1)?));
    }
    let mut first = None;
    let mut second = None;
    for (index, arg) in args.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_deref) {
            Some(name) if name == first_param => first = Some(arg),
            Some(name) if name == second_param => second = Some(arg),
            Some(_) => return None,
            None if first.is_none() => first = Some(arg),
            None if second.is_none() => second = Some(arg),
            None => return None,
        }
    }
    Some((first?, second?))
}

/// Tests whether a receiver type names the standard HTTP response type.
///
/// Inputs:
/// - `receiver_type`: normalized receiver type text from syntax lowering.
///
/// Output:
/// - `true` for `std.http.Response.Response` or its imported short name.
///
/// Transformation:
/// - Mirrors request receiver type detection so imported handler parameter
///   annotations can use the short `Response` name after a type import.
fn is_http_response_receiver_type(receiver_type: &str) -> bool {
    receiver_type_has_head(receiver_type, "std.http.Response.Response")
        || receiver_type_head(receiver_type) == "Response"
}

/// Reports whether an HTTP response helper mutates the response receiver.
///
/// Inputs:
/// - `receiver_type`: inferred source type for the receiver expression.
/// - `method`: response receiver helper name.
/// - `arg_count`: number of non-receiver arguments.
///
/// Output:
/// - `true` when direct response lowering can return an updated receiver.
///
/// Transformation:
/// - Gives mutable receiver sequence lowering the same closed response-helper
///   surface that ordinary direct response lowering already handles.
pub(in crate::backends::erlang::emit::syntax) fn is_http_response_mutating_receiver_method(
    receiver_type: &str,
    method: &str,
    arg_count: usize,
) -> bool {
    is_http_response_receiver_type(receiver_type)
        && matches!(
            (method, arg_count),
            ("status", 1)
                | ("header", 2)
                | ("set_cookie_header", 1)
                | ("cookie", 2..=5)
                | ("with_cookie", 2..=5)
                | ("cookie_with_options", 2..=10)
                | ("with_cookie_options", 2..=10)
                | ("delete_cookie", 1..=2)
                | ("with_deleted_cookie", 1..=2)
        )
}

/// Lowers a response status update.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `status`: source status expression.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `case` expression returning a response tuple with updated status.
///
/// Transformation:
/// - Supports both response ABI tuple shapes so status updates work before and
///   after headers have been appended.
fn lower_http_response_status_update(
    receiver: &SyntaxExprOutput,
    status: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let response = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    let status = lower_syntax_expr_with_env(status, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "case {response} of {{terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseBody}} -> {{terlan_response, {status}, _TerlanResponseContentType, _TerlanResponseBody}}; {{terlan_response, _TerlanResponseOldStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody}} -> {{terlan_response, {status}, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody}} end"
    )))
}

/// Lowers a response header append with source-provided header name.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `name`: source header name expression.
/// - `value`: source header value expression.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `case` expression returning a response tuple with one extra header.
///
/// Transformation:
/// - Normalizes header names and values to binaries and appends them to either
///   response ABI tuple shape.
fn lower_http_response_header_append(
    receiver: &SyntaxExprOutput,
    name: &SyntaxExprOutput,
    value: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let name = lower_syntax_expr_with_env(name, ctx, env)?.render();
    lower_http_response_header_append_literal_name(
        receiver,
        &format!("unicode:characters_to_binary({name})"),
        value,
        ctx,
        env,
    )
}

/// Lowers a response header append with a pre-rendered header name.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `name`: rendered Erlang header-name expression.
/// - `value`: source header value expression.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `case` expression returning a response tuple with one extra header.
///
/// Transformation:
/// - Centralizes header append rendering for ordinary headers and
///   `Set-Cookie`, preserving existing headers when the response already has
///   the five-tuple ABI shape.
fn lower_http_response_header_append_literal_name(
    receiver: &SyntaxExprOutput,
    name: &str,
    value: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let response = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    let value = lower_syntax_expr_with_env(value, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "case {response} of {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody}} -> {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{{{name}, unicode:characters_to_binary({value})}}], _TerlanResponseBody}}; {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody}} -> {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{{{name}, unicode:characters_to_binary({value})}}], _TerlanResponseBody}} end"
    )))
}

/// Lowers a response header append with pre-rendered name and value.
///
/// Inputs:
/// - `receiver`: response tuple expression.
/// - `name`: rendered Erlang header-name expression.
/// - `value`: rendered Erlang header-value expression.
/// - `ctx`, `env`: active syntax lowering context and local type environment.
///
/// Output:
/// - Erlang `case` expression returning a response tuple with one extra header.
///
/// Transformation:
/// - Supports direct helper lowering that has already assembled a backend
///   expression, such as a `Set-Cookie` value built from named/defaulted
///   helper arguments. The supplied value must already render to a binary.
fn lower_http_response_header_append_rendered_value(
    receiver: &SyntaxExprOutput,
    name: &str,
    value: &str,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let response = lower_syntax_expr_with_env(receiver, ctx, env)?.render();
    Some(ErlExpr::Raw(format!(
        "case {response} of {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseBody}} -> {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, [{{{name}, {value}}}], _TerlanResponseBody}}; {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders, _TerlanResponseBody}} -> {{terlan_response, _TerlanResponseStatus, _TerlanResponseContentType, _TerlanResponseHeaders ++ [{{{name}, {value}}}], _TerlanResponseBody}} end"
    )))
}
