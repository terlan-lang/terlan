use std::path::{Path, PathBuf};
use std::process::Command;

use terlan_safenative::http as native_http;

use crate::commands::web_route::{
    is_identifier, route_param_names, route_param_types, validate_route_pattern,
};

use super::manifest::read_web_manifest;
use super::{package_relative_path, RELOAD_ENDPOINT};

mod beam_eval;
mod response_bridge;
mod route;
mod types;

use beam_eval::{
    beam_ebin_dir_for_web_root, render_beam_error_handler_eval, render_beam_handler_eval,
};
use response_bridge::validate_response_header;
pub(super) use response_bridge::{static_response_header_tuples, BeamHandlerResponse};
use route::select_handler_for_request;
pub(super) use route::{validate_handler_routes, MatchedWebPackageHandler};
pub(super) use types::{
    WebPackageErrorHandler, WebPackageFileResponse, WebPackageHandler, WebPackageSourceSpan,
    WebPackageStaticResponse, WebPackageWebSocket,
};

/// Handler identity used by local request logs.
///
/// Inputs:
/// - Borrowed from a matched web package handler.
///
/// Output:
/// - Source-visible route and handler target metadata.
///
/// Transformation:
/// - Exposes only immutable identity fields needed by `terlc serve` logging
///   without making the matched route internals public.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HandlerLogIdentity<'a> {
    pub(super) method: &'a str,
    pub(super) route: &'a str,
    pub(super) module: &'a str,
    pub(super) function: &'a str,
    pub(super) source: Option<&'a WebPackageSourceSpan>,
}

/// Returns log identity for one matched handler.
///
/// Inputs:
/// - `matched`: selected dynamic route handler.
///
/// Output:
/// - Borrowed handler identity fields for logging.
///
/// Transformation:
/// - Reads manifest handler metadata while preserving route params and other
///   execution details inside the handler module.
pub(super) fn handler_log_identity(matched: &MatchedWebPackageHandler) -> HandlerLogIdentity<'_> {
    HandlerLogIdentity {
        method: &matched.handler.method,
        route: &matched.handler.route,
        module: &matched.handler.module,
        function: &matched.handler.function,
        source: matched.handler.source.as_ref(),
    }
}

/// Validates one dynamic HTTP handler manifest entry.
///
/// Inputs:
/// - `handler`: manifest-declared route and Terlan function target.
///
/// Output:
/// - `Ok(())` when the handler entry is safe and supported.
/// - `Err(String)` with a stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Checks route shape, allowed HTTP method, module/function spelling, and
///   handler arity before the server reserves the route.
pub(super) fn validate_handler(handler: &WebPackageHandler) -> Result<(), String> {
    validate_handler_method(&handler.method)?;
    validate_handler_route(&handler.route)?;
    validate_handler_module(&handler.module)?;
    validate_handler_function(&handler.function)?;
    if let Some(source) = &handler.source {
        validate_source_span(
            "handler",
            &format!("{}.{}", handler.module, handler.function),
            source,
        )?;
    }
    let route_param_count = route_param_names(&handler.route)?.len();
    let expected_with_params = 1 + route_param_count;
    if handler.arity != 1 && handler.arity != expected_with_params {
        return Err(format!(
            "error[serve_package]: handler `{}` `{}` must have arity 1 for Request input or arity {} for Request plus route parameter(s), got {}",
            handler.method, handler.route, expected_with_params, handler.arity
        ));
    }
    Ok(())
}

/// Validates one WebSocket manifest route.
///
/// Inputs:
/// - `websocket`: manifest-declared socket route and protocol identity.
///
/// Output:
/// - `Ok(())` when the route and protocol are safe.
/// - Stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Reuses HTTP route pattern validation for upgrade paths and constrains the
///   protocol name to the first runtime-owned protocol supported by local
///   serve.
pub(super) fn validate_websocket(websocket: &WebPackageWebSocket) -> Result<(), String> {
    validate_handler_route(&websocket.route)?;
    if websocket.protocol != "battleship.room.v1" {
        return Err(format!(
            "error[serve_package]: websocket `{}` uses unsupported protocol `{}`",
            websocket.route, websocket.protocol
        ));
    }
    if let Some(source) = &websocket.source {
        validate_source_span(
            "websocket",
            &format!("{} {}", websocket.protocol, websocket.route),
            source,
        )?;
    }
    Ok(())
}

/// Validates optional source metadata attached to a handler manifest entry.
///
/// Inputs:
/// - `kind`: manifest row kind for diagnostics.
/// - `identity`: source-visible row identity for diagnostics.
/// - `source`: source metadata supplied by the generated manifest.
///
/// Output:
/// - `Ok(())` when the source path and span are safe.
/// - Stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Keeps source metadata project-relative and one-based before it can appear
///   in local logs or development error pages.
fn validate_source_span(
    kind: &str,
    identity: &str,
    source: &WebPackageSourceSpan,
) -> Result<(), String> {
    let path = Path::new(&source.path);
    if source.path.trim().is_empty()
        || source.path.contains('\\')
        || source.path.contains('\0')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "error[serve_package]: {kind} `{identity}` has unsafe source path `{}`",
            source.path
        ));
    }
    if source.line == 0 || source.column == 0 {
        return Err(format!(
            "error[serve_package]: {kind} `{identity}` source span must use one-based line and column"
        ));
    }
    Ok(())
}

/// Validates one router-level error handler manifest entry.
///
/// Inputs:
/// - `handler`: manifest-declared Terlan function target.
///
/// Output:
/// - `Ok(())` when the handler identity is safe and arity is supported.
/// - `Err(String)` with a stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Reuses module/function spelling checks from normal route handlers while
///   enforcing the single `HttpError` input expected by `std.http.Router.error`.
pub(super) fn validate_error_handler(handler: &WebPackageErrorHandler) -> Result<(), String> {
    validate_handler_module(&handler.module)?;
    validate_handler_function(&handler.function)?;
    if handler.arity != 1 {
        return Err(format!(
            "error[serve_package]: error handler `{}.{}` must have arity 1 for HttpError input, got {}",
            handler.module, handler.function, handler.arity
        ));
    }
    Ok(())
}

/// Validates one static response manifest entry.
///
/// Inputs:
/// - `response`: manifest-declared static response row.
///
/// Output:
/// - `Ok(())` when the method, route, status, content type, and body are safe.
/// - Stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Reuses route/method validation from dynamic handlers and adds the smaller
///   literal response checks needed before a server can emit the row directly.
pub(super) fn validate_static_response(response: &WebPackageStaticResponse) -> Result<(), String> {
    validate_handler_method(&response.method)?;
    validate_handler_route(&response.route)?;
    if !(100..=599).contains(&response.status) {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` has invalid status `{}`",
            response.method, response.route, response.status
        ));
    }
    if response.content_type.trim().is_empty()
        || response
            .content_type
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n')
    {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` has invalid content type",
            response.method, response.route
        ));
    }
    for header in &response.headers {
        validate_response_header(&header.name, &header.value).map_err(|message| {
            format!(
                "error[serve_package]: static response `{}` `{}` has invalid header: {message}",
                response.method, response.route
            )
        })?;
    }
    if let Some(source) = &response.source {
        validate_source_span(
            "static response",
            &format!("{} {}", response.method, response.route),
            source,
        )?;
    }
    Ok(())
}

/// Validates one file response manifest entry.
///
/// Inputs:
/// - `response`: manifest-declared file response row.
///
/// Output:
/// - `Ok(())` when the method, route, status, and optional content type are
///   safe.
/// - Stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Reuses route/method validation from dynamic handlers and leaves
///   filesystem existence checks to the package validator, which has the
///   package root.
pub(super) fn validate_file_response(response: &WebPackageFileResponse) -> Result<(), String> {
    validate_handler_method(&response.method)?;
    validate_handler_route(&response.route)?;
    if response.path.trim().is_empty()
        || response.path.contains('\\')
        || response.path.contains('\0')
        || Path::new(&response.path).is_absolute()
    {
        return Err(format!(
            "error[serve_package]: file response `{}` `{}` has unsafe path `{}`",
            response.method, response.route, response.path
        ));
    }
    if !(100..=599).contains(&response.status) {
        return Err(format!(
            "error[serve_package]: file response `{}` `{}` has invalid status `{}`",
            response.method, response.route, response.status
        ));
    }
    if let Some(content_type) = &response.content_type {
        if content_type.trim().is_empty()
            || content_type
                .bytes()
                .any(|byte| byte == b'\r' || byte == b'\n')
        {
            return Err(format!(
                "error[serve_package]: file response `{}` `{}` has invalid content type",
                response.method, response.route
            ));
        }
    }
    if let Some(source) = &response.source {
        validate_source_span(
            "file response",
            &format!("{} {}", response.method, response.route),
            source,
        )?;
    }
    Ok(())
}

/// Validates a handler HTTP method.
///
/// Inputs:
/// - `method`: manifest-declared method text.
///
/// Output:
/// - `Ok(())` for methods accepted by the local handler contract.
/// - `Err(String)` for unsupported methods.
///
/// Transformation:
/// - Restricts dynamic handler declarations to the HTTP methods generated by
///   `std.http.Router` manifest extraction.
fn validate_handler_method(method: &str) -> Result<(), String> {
    match method {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => Ok(()),
        other => Err(format!(
            "error[serve_package]: unsupported handler method `{other}`"
        )),
    }
}

/// Validates a handler route path.
///
/// Inputs:
/// - `route`: manifest-declared URL path.
///
/// Output:
/// - `Ok(())` for safe absolute route paths and the canonical `*` fallback.
/// - `Err(String)` for traversal, query strings, fragments, or reserved paths.
///
/// Transformation:
/// - Applies URL-route safety checks separate from filesystem path handling so
///   dynamic routes cannot escape into package file lookup semantics.
fn validate_handler_route(route: &str) -> Result<(), String> {
    if route == "*" {
        return validate_route_pattern(route);
    }
    if !route.starts_with('/') || route.contains('\\') || route.contains('\0') {
        return Err(format!(
            "error[serve_package]: unsafe handler route `{route}`"
        ));
    }
    if route.contains('?') || route.contains('#') {
        return Err(format!(
            "error[serve_package]: handler route `{route}` must not contain query or fragment text"
        ));
    }
    if route == RELOAD_ENDPOINT {
        return Err(format!(
            "error[serve_package]: handler route `{route}` is reserved for live reload"
        ));
    }
    validate_route_pattern(route)?;
    Ok(())
}

/// Validates a Terlan module path in a handler target.
///
/// Inputs:
/// - `module`: manifest-declared Terlan module path.
///
/// Output:
/// - `Ok(())` when each dot-separated segment is a Terlan-style identifier.
/// - `Err(String)` otherwise.
///
/// Transformation:
/// - Performs a small lexical validation so malformed manifests fail before
///   runtime dispatch tries to resolve a module.
fn validate_handler_module(module: &str) -> Result<(), String> {
    if module
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifier(segment))
    {
        Ok(())
    } else {
        Err(format!(
            "error[serve_package]: invalid handler module `{module}`"
        ))
    }
}

/// Validates a Terlan function name in a handler target.
///
/// Inputs:
/// - `function`: manifest-declared Terlan function name.
///
/// Output:
/// - `Ok(())` for a lowercase identifier.
/// - `Err(String)` otherwise.
///
/// Transformation:
/// - Keeps handler dispatch targets aligned with Terlan function naming.
fn validate_handler_function(function: &str) -> Result<(), String> {
    if is_identifier(function)
        && function
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
    {
        Ok(())
    } else {
        Err(format!(
            "error[serve_package]: invalid handler function `{function}`"
        ))
    }
}

/// Finds a manifest-declared handler for one request.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `method`: parsed HTTP request method.
/// - `request_path`: parsed URL path without query text.
///
/// Output:
/// - Matching handler when the manifest declares one.
/// - `None` when there is no manifest, no matching route, or the route is
///   static-only.
///
/// Transformation:
/// - Reads the manifest and performs exact method/path matching. `HEAD`
///   requests may use `GET` handlers because they share route metadata while
///   suppressing response bodies.
pub(super) fn manifest_handler_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<MatchedWebPackageHandler> {
    let manifest = read_web_manifest(web_root).ok()?;
    select_handler_for_request(manifest.handlers, method, request_path)
}

/// Finds a manifest-declared static response for one request.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `method`: parsed HTTP request method.
/// - `request_path`: parsed URL path without query text.
///
/// Output:
/// - Matching static response when the manifest declares one.
/// - `None` when no static response route matches.
///
/// Transformation:
/// - Reuses the dynamic route selector by projecting static responses into
///   temporary route candidates, preserving exact/parameter/wildcard/fallback
///   precedence and `HEAD` to `GET` fallback behavior.
pub(super) fn manifest_static_response_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<WebPackageStaticResponse> {
    let manifest = read_web_manifest(web_root).ok()?;
    let candidates = manifest
        .static_responses
        .iter()
        .map(|response| WebPackageHandler {
            method: response.method.clone(),
            route: response.route.clone(),
            module: "static".to_string(),
            function: "response".to_string(),
            arity: 1,
            source: response.source.clone(),
        })
        .collect();
    let matched = select_handler_for_request(candidates, method, request_path)?;
    manifest.static_responses.into_iter().find(|response| {
        response.method == matched.handler.method && response.route == matched.handler.route
    })
}

/// Finds a manifest-declared file response for one request.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `method`: parsed HTTP request method.
/// - `request_path`: parsed URL path without query text.
///
/// Output:
/// - Matching file response plus resolved package file path when declared.
/// - `None` when no file response route matches or the resolved path is unsafe
///   or missing.
///
/// Transformation:
/// - Reuses the dynamic route selector by projecting file responses into
///   temporary route candidates, preserving exact/parameter/wildcard/fallback
///   precedence and `HEAD` to `GET` fallback behavior.
pub(super) fn manifest_file_response_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<(WebPackageFileResponse, PathBuf)> {
    let manifest = read_web_manifest(web_root).ok()?;
    let candidates = manifest
        .file_responses
        .iter()
        .map(|response| WebPackageHandler {
            method: response.method.clone(),
            route: response.route.clone(),
            module: "static".to_string(),
            function: "file".to_string(),
            arity: 1,
            source: response.source.clone(),
        })
        .collect();
    let matched = select_handler_for_request(candidates, method, request_path)?;
    let response = manifest.file_responses.into_iter().find(|response| {
        response.method == matched.handler.method && response.route == matched.handler.route
    })?;
    let path = package_relative_path(web_root, &response.path)?;
    path.is_file().then_some((response, path))
}

/// Finds the manifest-declared router error handler.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
///
/// Output:
/// - Error handler metadata when the manifest declares one.
/// - `None` when no manifest exists or no error handler is declared.
///
/// Transformation:
/// - Reads the web package manifest and extracts only the optional
///   source-visible `HttpError -> Response` callback identity.
pub(super) fn manifest_error_handler(web_root: &Path) -> Option<WebPackageErrorHandler> {
    read_web_manifest(web_root).ok()?.error_handler
}

/// Executes a manifest-declared handler through the generated BEAM artifacts.
///
/// Inputs:
/// - `web_root`: package root, normally `_build/web`.
/// - `handler`: validated manifest handler target.
/// - `method`: request method as parsed from the HTTP request line.
/// - `request_path`: request URL path without query text.
/// - `request_query`: raw query text without leading `?`.
/// - `headers`: normalized request header pairs.
/// - `cookie_header`: raw `Cookie` request header value.
/// - `request_body`: buffered request body text.
///
/// Output:
/// - Parsed handler response when BEAM execution succeeds.
/// - Stable `error[serve_handler]` text when artifacts, `erl`, execution, or
///   handler return shape are invalid.
///
/// Transformation:
/// - Resolves the sibling `_build/ebin` directory, invokes `erl -noshell` with
///   the generated BEAM code path, passes a small request map, and parses the
///   stable `{terlan_response, Status, ContentType, Body}` or
///   `{terlan_response, Status, ContentType, Headers, Body}` ABI printed by the
///   Erlang runner expression.
pub(super) fn execute_beam_handler(
    web_root: &Path,
    matched: &MatchedWebPackageHandler,
    method: &str,
    request_path: &str,
    request_query: &str,
    headers: &[(String, String)],
    cookie_header: &str,
    request_body: &str,
) -> Result<BeamHandlerResponse, String> {
    let ebin_dir = beam_ebin_dir_for_web_root(web_root)?;
    if !ebin_dir.is_dir() {
        return Err(format!(
            "error[serve_handler]: BEAM ebin directory `{}` does not exist; run `terlc build --target erlang` for handler modules",
            ebin_dir.display()
        ));
    }

    let handler = &matched.handler;
    let erlang_module = crate::support::erlang_output_stem(&handler.module);
    let beam_path = ebin_dir.join(format!("{erlang_module}.beam"));
    if !beam_path.is_file() {
        return Err(format!(
            "error[serve_handler]: BEAM module `{}` for handler `{}` `{}` was not found at `{}`",
            handler.module,
            handler.method,
            handler.route,
            beam_path.display()
        ));
    }

    let request = native_http::Request::from_parts(method, request_path, request_body);
    let route_param_types = route_param_types(&handler.route)?;
    let eval = render_beam_handler_eval(
        &erlang_module,
        &handler.function,
        &request,
        &matched.params,
        &route_param_types,
        handler.arity,
        request_query,
        headers,
        cookie_header,
    );
    let output = Command::new("erl")
        .arg("-noshell")
        .arg("-pa")
        .arg(&ebin_dir)
        .arg("-eval")
        .arg(eval)
        .env("TERLAN_SQL_RUNTIME_HELPER", current_terlc_helper()?)
        .current_dir(&ebin_dir)
        .output()
        .map_err(|err| format!("error[serve_handler]: failed to run `erl`: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        let detail = if detail.is_empty() {
            format!("erl exited with status {}", output.status)
        } else {
            detail.to_string()
        };
        return Err(format!(
            "error[serve_handler]: handler `{}.{}/{}` failed through BEAM: {detail}",
            handler.module, handler.function, handler.arity
        ));
    }

    parse_beam_handler_stdout(&output.stdout)
}

/// Executes a manifest-declared router error handler through BEAM artifacts.
///
/// Inputs:
/// - `web_root`: package root, normally `_build/web`.
/// - `handler`: validated router error handler target.
/// - `message`: source-aware diagnostic from the failed route handler.
///
/// Output:
/// - Parsed handler response when BEAM execution succeeds.
/// - Stable `error[serve_handler]` text when artifacts, `erl`, execution, or
///   handler return shape are invalid.
///
/// Transformation:
/// - Resolves the sibling `_build/ebin` directory, invokes the generated
///   error handler with a portable `HttpError` record tuple, and parses the
///   same response ABI used by ordinary route handlers.
pub(super) fn execute_beam_error_handler(
    web_root: &Path,
    handler: &WebPackageErrorHandler,
    message: &str,
) -> Result<BeamHandlerResponse, String> {
    let ebin_dir = beam_ebin_dir_for_web_root(web_root)?;
    if !ebin_dir.is_dir() {
        return Err(format!(
            "error[serve_handler]: BEAM ebin directory `{}` does not exist; run `terlc build --target erlang` for handler modules",
            ebin_dir.display()
        ));
    }

    let erlang_module = crate::support::erlang_output_stem(&handler.module);
    let beam_path = ebin_dir.join(format!("{erlang_module}.beam"));
    if !beam_path.is_file() {
        return Err(format!(
            "error[serve_handler]: BEAM module `{}` for error handler was not found at `{}`",
            handler.module,
            beam_path.display()
        ));
    }

    let eval = render_beam_error_handler_eval(&erlang_module, &handler.function, message);
    let output = Command::new("erl")
        .arg("-noshell")
        .arg("-pa")
        .arg(&ebin_dir)
        .arg("-eval")
        .arg(eval)
        .env("TERLAN_SQL_RUNTIME_HELPER", current_terlc_helper()?)
        .current_dir(&ebin_dir)
        .output()
        .map_err(|err| format!("error[serve_handler]: failed to run `erl`: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        let detail = if detail.is_empty() {
            format!("erl exited with status {}", output.status)
        } else {
            detail.to_string()
        };
        return Err(format!(
            "error[serve_handler]: error handler `{}.{}/1` failed through BEAM: {detail}",
            handler.module, handler.function
        ));
    }

    parse_beam_handler_stdout(&output.stdout)
}

/// Resolves the current `terlc` executable for handler trampoline calls.
///
/// Inputs:
/// - Process executable metadata from the operating system.
///
/// Output:
/// - Path to the running compiler binary or a stable serve-handler error.
///
/// Transformation:
/// - Converts `current_exe` failures into user-facing handler diagnostics.
fn current_terlc_helper() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|err| format!("error[serve_handler]: failed to resolve current terlc: {err}"))
}

/// Parses stdout from the BEAM handler runner.
///
/// Inputs:
/// - `stdout`: bytes written by the Erlang runner expression.
///
/// Output:
/// - Parsed handler response or stable serve-handler error text.
///
/// Transformation:
/// - Reads the first line as status, the second line as content type, then
///   parses an optional `#terlan-headers:N` section before preserving all
///   remaining bytes as the response body.
fn parse_beam_handler_stdout(stdout: &[u8]) -> Result<BeamHandlerResponse, String> {
    let first_newline = stdout
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| {
            "error[serve_handler]: BEAM handler response missing status line".to_string()
        })?;
    let status_text = std::str::from_utf8(&stdout[..first_newline])
        .map_err(|err| format!("error[serve_handler]: BEAM handler status is not UTF-8: {err}"))?;
    let status = status_text.trim().parse::<u16>().map_err(|err| {
        format!("error[serve_handler]: BEAM handler status `{status_text}` is invalid: {err}")
    })?;
    if !(100..=599).contains(&status) {
        return Err(format!(
            "error[serve_handler]: BEAM handler status `{status}` is outside HTTP range"
        ));
    }

    let rest = &stdout[first_newline + 1..];
    let second_newline = rest.iter().position(|byte| *byte == b'\n').ok_or_else(|| {
        "error[serve_handler]: BEAM handler response missing content-type line".to_string()
    })?;
    let content_type = std::str::from_utf8(&rest[..second_newline]).map_err(|err| {
        format!("error[serve_handler]: BEAM handler content type is not UTF-8: {err}")
    })?;
    let content_type = content_type.trim().to_string();
    if content_type.is_empty() {
        return Err("error[serve_handler]: BEAM handler content type is empty".to_string());
    }

    let (headers, body_bytes) = parse_optional_beam_response_headers(&rest[second_newline + 1..])?;
    let body = String::from_utf8(body_bytes)
        .map_err(|err| format!("error[serve_handler]: BEAM handler body is not UTF-8: {err}"))?;
    let mut native_response = native_http::Response::from_parts(status as i64, content_type, body);
    for (name, value) in headers {
        native_http::header(&mut native_response, &name, &value);
    }
    BeamHandlerResponse::from_native_response(&native_response)
}

/// Parses the optional BEAM response header section.
///
/// Inputs:
/// - `rest`: response bytes after status and content-type lines.
///
/// Output:
/// - Parsed response headers and body bytes.
///
/// Transformation:
/// - Preserves backward compatibility with the original three-line protocol by
///   treating missing `#terlan-headers:` marker text as body bytes.
fn parse_optional_beam_response_headers(
    rest: &[u8],
) -> Result<(Vec<(String, String)>, Vec<u8>), String> {
    const MARKER: &[u8] = b"#terlan-headers:";
    if !rest.starts_with(MARKER) {
        return Ok((Vec::new(), rest.to_vec()));
    }
    let marker_newline = rest.iter().position(|byte| *byte == b'\n').ok_or_else(|| {
        "error[serve_handler]: BEAM handler response header marker is incomplete".to_string()
    })?;
    let count_text = std::str::from_utf8(&rest[MARKER.len()..marker_newline]).map_err(|err| {
        format!("error[serve_handler]: BEAM handler header count is not UTF-8: {err}")
    })?;
    let count = count_text.trim().parse::<usize>().map_err(|err| {
        format!("error[serve_handler]: BEAM handler header count `{count_text}` is invalid: {err}")
    })?;
    let mut headers = Vec::with_capacity(count);
    let mut offset = marker_newline + 1;
    for _ in 0..count {
        let relative_newline = rest[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or_else(|| {
                "error[serve_handler]: BEAM handler response ended before declared headers"
                    .to_string()
            })?;
        let line = &rest[offset..offset + relative_newline];
        let line = std::str::from_utf8(line).map_err(|err| {
            format!("error[serve_handler]: BEAM handler header line is not UTF-8: {err}")
        })?;
        let (name, value) = line.split_once('\t').ok_or_else(|| {
            format!("error[serve_handler]: BEAM handler header line `{line}` is missing a tab delimiter")
        })?;
        headers.push((name.to_string(), value.to_string()));
        offset += relative_newline + 1;
    }
    Ok((headers, rest[offset..].to_vec()))
}

/// Returns a basic HTTP reason phrase for a status code.
///
/// Inputs:
/// - `status`: numeric HTTP status.
///
/// Output:
/// - Common reason phrase, or `OK` for unknown success and `Error` otherwise.
///
/// Transformation:
/// - Keeps handler-generated status lines valid without making the local
///   server depend on a full HTTP framework.
pub(super) fn http_reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ if status < 400 => "OK",
        _ => "Error",
    }
}

#[cfg(test)]
#[path = "handler_test.rs"]
mod handler_test;
