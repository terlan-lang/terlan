use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::terlan_native::http as native_http;
use url::form_urlencoded;

/// Resolves the BEAM output directory for a web package root.
///
/// Inputs:
/// - `web_root`: package root passed to `terlc serve`.
///
/// Output:
/// - Sibling `ebin` directory under the build root.
/// - Stable error if the package root has no parent.
///
/// Transformation:
/// - Treats `_build/web` as one view of the same build root that also owns
///   `_build/ebin`.
pub(super) fn beam_ebin_dir_for_web_root(web_root: &Path) -> Result<PathBuf, String> {
    let build_root = web_root.parent().ok_or_else(|| {
        format!(
            "error[serve_handler]: cannot resolve build root for web package `{}`",
            web_root.display()
        )
    })?;
    Ok(build_root.join("ebin"))
}

/// Renders the Erlang expression used to invoke one handler.
///
/// Inputs:
/// - `erlang_module`: generated Erlang module atom text.
/// - `function`: generated Erlang function atom text.
/// - `request`: Rust-native request snapshot to bridge into BEAM.
/// - `params`: decoded route params captured by the route matcher.
/// - `route_param_types`: route-declared capture names and types.
/// - `handler_arity`: manifest-declared handler arity.
/// - `query_string`: raw query text without leading `?`.
/// - `headers`: normalized request header pairs.
/// - `cookie_header`: raw `Cookie` request header value.
///
/// Output:
/// - Erlang `-eval` source that exits zero only for the stable response ABI.
///
/// Transformation:
/// - Builds a small request map and converts the handler return value into a
///   stdout protocol with status, content type, optional response headers, and
///   raw body bytes. Handlers may accept only `Request` or `Request` followed
///   by route captures in route order. Supported typed captures are decoded
///   before handler invocation.
pub(super) fn render_beam_handler_eval(
    erlang_module: &str,
    function: &str,
    request: &native_http::Request,
    params: &[(String, String)],
    route_param_types: &[(String, String)],
    handler_arity: usize,
    query_string: &str,
    headers: &[(String, String)],
    cookie_header: &str,
) -> String {
    let method = erlang_binary_literal(request.method().as_bytes());
    let path = erlang_binary_literal(request.path().as_bytes());
    let body = erlang_binary_literal(request.body().as_bytes());
    let params_map = erlang_binary_map_literal(params);
    let query_string_literal = erlang_binary_literal(query_string.as_bytes());
    let query = erlang_binary_map_literal(&parse_query_params(query_string));
    let headers = erlang_binary_map_literal(headers);
    let cookie_header_literal = erlang_binary_literal(cookie_header.as_bytes());
    let cookies =
        erlang_binary_map_literal(&native_http::parse_request_cookie_header(cookie_header));
    let handler_args = erlang_handler_call_args(params, route_param_types, handler_arity);
    format!(
        "Request = #{{method => {method}, path => {path}, body => {body}, params => {params_map}, query_string => {query_string_literal}, query => {query}, headers => {headers}, cookie_header => {cookie_header_literal}, cookies => {cookies}}}, Result = catch {erlang_module}:{function}({handler_args}), Print = fun(Status, ContentType, Headers, Body) -> io:format(\"~B~n~ts~n#terlan-headers:~B~n\", [Status, ContentType, length(Headers)]), lists:foreach(fun({{Name, Value}}) -> io:format(\"~ts\\t~ts~n\", [Name, Value]) end, Headers), io:put_chars(Body), halt(0) end, case Result of {{terlan_response, Status, ContentType, Body}} when is_integer(Status), is_binary(ContentType), is_binary(Body) -> Print(Status, ContentType, [], Body); {{terlan_response, Status, ContentType, Headers, Body}} when is_integer(Status), is_binary(ContentType), is_list(Headers), is_binary(Body) -> case lists:all(fun({{Name, Value}}) when is_binary(Name), is_binary(Value) -> true; (_) -> false end, Headers) of true -> Print(Status, ContentType, Headers, Body); false -> io:format(standard_error, \"handler returned unsupported response headers: ~p~n\", [Headers]), halt(12) end; {{'EXIT', Reason}} -> io:format(standard_error, \"handler failed: ~p~n\", [Reason]), halt(11); Other -> io:format(standard_error, \"handler returned unsupported value: ~p~n\", [Other]), halt(12) end."
    )
}

/// Renders the Erlang handler call argument list.
///
/// Inputs:
/// - `params`: decoded route params in route order.
/// - `route_param_types`: route-declared capture names and types.
/// - `handler_arity`: manifest-declared BEAM handler arity.
///
/// Output:
/// - Erlang call argument text such as `Request` or `Request, <<52,50>>`.
///
/// Transformation:
/// - Keeps `Request` as the first argument and appends captured route values
///   in their route-declared type only when the generated manifest says the
///   handler accepts route parameters directly.
fn erlang_handler_call_args(
    params: &[(String, String)],
    route_param_types: &[(String, String)],
    handler_arity: usize,
) -> String {
    if handler_arity <= 1 {
        return "Request".to_string();
    }
    let mut args = vec!["Request".to_string()];
    args.extend(
        params
            .iter()
            .take(handler_arity.saturating_sub(1))
            .map(|(name, value)| {
                let type_name = route_param_types
                    .iter()
                    .find_map(|(route_name, type_name)| (route_name == name).then_some(type_name))
                    .map(String::as_str)
                    .unwrap_or("String");
                erlang_route_param_value(name, type_name, value)
            }),
    );
    args.join(", ")
}

/// Renders one route parameter value for BEAM handler invocation.
///
/// Inputs:
/// - `name`: route capture name.
/// - `type_name`: route-declared capture type.
/// - `value`: decoded route capture value.
///
/// Output:
/// - Erlang expression for the handler argument.
///
/// Transformation:
/// - Leaves `String` captures as binaries and decodes `Int`/`Bool` captures
///   with guards that raise source-readable route-param errors when the URL
///   segment cannot be converted.
fn erlang_route_param_value(name: &str, type_name: &str, value: &str) -> String {
    let binary = erlang_binary_literal(value.as_bytes());
    match final_type_segment(type_name) {
        "Int" => {
            let name = erlang_binary_literal(name.as_bytes());
            format!(
                "(case string:to_integer(binary_to_list({binary})) of {{Value, []}} -> Value; _ -> erlang:error({{invalid_route_param, {name}, <<\"Int\">>, {binary}}}) end)"
            )
        }
        "Bool" => {
            let name = erlang_binary_literal(name.as_bytes());
            format!(
                "(case {binary} of <<\"true\">> -> true; <<\"false\">> -> false; _ -> erlang:error({{invalid_route_param, {name}, <<\"Bool\">>, {binary}}}) end)"
            )
        }
        _ => binary,
    }
}

/// Returns the final dot-qualified type segment.
///
/// Inputs:
/// - `type_name`: source or route type text.
///
/// Output:
/// - Final type segment.
///
/// Transformation:
/// - Keeps route-param decoding independent from full compiler type
///   resolution while still accepting qualified type names.
fn final_type_segment(type_name: &str) -> &str {
    type_name
        .trim()
        .rsplit('.')
        .next()
        .unwrap_or(type_name.trim())
}

/// Renders the Erlang expression used to invoke a router error handler.
///
/// Inputs:
/// - `erlang_module`: generated Erlang module atom text.
/// - `function`: generated Erlang function atom text.
/// - `message`: source-aware handler failure diagnostic.
///
/// Output:
/// - Erlang `-eval` source that calls `function(HttpError)` and prints the
///   stable response ABI.
///
/// Transformation:
/// - Builds the current BEAM record tuple for `std.http.Error.HttpError` with
///   code, message, and status fields, then reuses the same stdout protocol as
///   normal request handlers.
pub(super) fn render_beam_error_handler_eval(
    erlang_module: &str,
    function: &str,
    message: &str,
) -> String {
    let code = "serve_handler_execution_failed";
    let message = erlang_binary_literal(message.as_bytes());
    format!(
        "Error = {{http_error, {code}, {message}, 502}}, Result = catch {erlang_module}:{function}(Error), Print = fun(Status, ContentType, Headers, Body) -> io:format(\"~B~n~ts~n#terlan-headers:~B~n\", [Status, ContentType, length(Headers)]), lists:foreach(fun({{Name, Value}}) -> io:format(\"~ts\\t~ts~n\", [Name, Value]) end, Headers), io:put_chars(Body), halt(0) end, case Result of {{terlan_response, Status, ContentType, Body}} when is_integer(Status), is_binary(ContentType), is_binary(Body) -> Print(Status, ContentType, [], Body); {{terlan_response, Status, ContentType, Headers, Body}} when is_integer(Status), is_binary(ContentType), is_list(Headers), is_binary(Body) -> case lists:all(fun({{Name, Value}}) when is_binary(Name), is_binary(Value) -> true; (_) -> false end, Headers) of true -> Print(Status, ContentType, Headers, Body); false -> io:format(standard_error, \"error handler returned unsupported response headers: ~p~n\", [Headers]), halt(12) end; {{'EXIT', Reason}} -> io:format(standard_error, \"error handler failed: ~p~n\", [Reason]), halt(11); Other -> io:format(standard_error, \"error handler returned unsupported value: ~p~n\", [Other]), halt(12) end."
    )
}

/// Parses URL query text for the temporary BEAM handler bridge.
///
/// Inputs:
/// - `query_string`: raw query text without a leading `?`.
///
/// Output:
/// - Decoded key/value pairs for the handler request map.
///
/// Transformation:
/// - Delegates form/query percent decoding to the `url` crate. Repeated keys
///   intentionally collapse to the last value when rendered through the bridge
///   map; the later typed `std.http.Query` API can expose richer multi-value
///   behavior.
pub(super) fn parse_query_params(query_string: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(query_string.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

/// Renders route params as an Erlang map with binary keys and values.
///
/// Inputs:
/// - `params`: decoded route params captured by the route matcher.
///
/// Output:
/// - Erlang map literal suitable for embedding in the request map.
///
/// Transformation:
/// - Sorts by key for deterministic generated `erl -eval` text and renders
///   exact bytes as binary literals.
fn erlang_binary_map_literal(params: &[(String, String)]) -> String {
    if params.is_empty() {
        return "#{}".to_string();
    }
    let ordered: BTreeMap<&str, &str> = params
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let entries = ordered
        .into_iter()
        .map(|(key, value)| {
            let key = erlang_binary_literal(key.as_bytes());
            let value = erlang_binary_literal(value.as_bytes());
            format!("{key} => {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("#{{{entries}}}")
}

/// Renders bytes as an Erlang binary literal.
///
/// Inputs:
/// - `bytes`: exact byte sequence to embed.
///
/// Output:
/// - Erlang binary syntax such as `<<47,97,112,105>>`.
///
/// Transformation:
/// - Uses numeric bytes instead of string escapes so request data cannot break
///   the generated `erl -eval` expression.
fn erlang_binary_literal(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<<>>".to_string();
    }
    let body = bytes
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("<<{body}>>")
}
