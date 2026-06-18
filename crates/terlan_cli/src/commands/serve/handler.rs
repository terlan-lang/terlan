use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use terlan_safenative::http as native_http;

use super::manifest::read_web_manifest;
use super::RELOAD_ENDPOINT;

/// One dynamic handler entry inside the browser package manifest.
///
/// Inputs:
/// - Deserialized from `_build/web/manifest.json`.
///
/// Output:
/// - Route method/path and Terlan module/function identity reserved for
///   BEAM-backed handler dispatch.
///
/// Transformation:
/// - Keeps dynamic routes declarative in the package manifest so the local
///   server does not hard-code application route behavior.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct WebPackageHandler {
    pub(super) method: String,
    pub(super) route: String,
    pub(super) module: String,
    pub(super) function: String,
    pub(super) arity: usize,
}

/// HTTP response returned by a BEAM-backed handler.
///
/// Inputs:
/// - Produced by the handler runner after parsing the stable handler ABI.
///
/// Output:
/// - Status, content type, and byte body ready for the local HTTP writer.
///
/// Transformation:
/// - Keeps BEAM process output separate from socket writing so handler
///   execution can be tested without binding a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BeamHandlerResponse {
    pub(super) status: u16,
    pub(super) content_type: String,
    pub(super) body: Vec<u8>,
}

impl BeamHandlerResponse {
    /// Converts a Rust-native HTTP adapter response into the server wire shape.
    ///
    /// Inputs:
    /// - `response`: Rust-native `std.http.Response` backing value.
    ///
    /// Output:
    /// - Internal handler response accepted by the local HTTP writer.
    /// - Stable serve-handler error when the status cannot be represented as
    ///   an HTTP status code.
    ///
    /// Transformation:
    /// - Copies native response status, content type, and UTF-8 body into the
    ///   bridge response without exposing the BEAM tuple ABI to source code.
    fn from_native_response(response: &native_http::Response) -> Result<Self, String> {
        let status = response.status_code();
        if !(100..=599).contains(&status) {
            return Err(format!(
                "error[serve_handler]: native HTTP response status `{status}` is outside HTTP range"
            ));
        }
        Ok(Self {
            status: status as u16,
            content_type: response.content_type().to_string(),
            body: response.body().as_bytes().to_vec(),
        })
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
    if handler.arity != 1 {
        return Err(format!(
            "error[serve_package]: handler `{}` `{}` must have arity 1 for Request input, got {}",
            handler.method, handler.route, handler.arity
        ));
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
/// - Restricts dynamic handler declarations to common methods the request
///   parser can identify without committing to a framework-specific router.
fn validate_handler_method(method: &str) -> Result<(), String> {
    match method {
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" => Ok(()),
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
/// - `Ok(())` for safe absolute route paths.
/// - `Err(String)` for traversal, query strings, fragments, or reserved paths.
///
/// Transformation:
/// - Applies URL-route safety checks separate from filesystem path handling so
///   dynamic routes cannot escape into package file lookup semantics.
fn validate_handler_route(route: &str) -> Result<(), String> {
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
    for component in Path::new(route.trim_start_matches('/')).components() {
        if matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return Err(format!(
                "error[serve_package]: unsafe handler route `{route}`"
            ));
        }
    }
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

/// Returns whether text is an ASCII Terlan identifier.
///
/// Inputs:
/// - `value`: candidate identifier text.
///
/// Output:
/// - `true` when the text starts with an ASCII letter or underscore and
///   continues with ASCII letters, digits, or underscores.
///
/// Transformation:
/// - Applies the manifest-level identifier subset needed for route targets
///   without invoking the full source parser.
fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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
) -> Option<WebPackageHandler> {
    let manifest = read_web_manifest(web_root).ok()?;
    manifest.handlers.into_iter().find(|handler| {
        handler.route == request_path
            && (handler.method == method || (method == "HEAD" && handler.method == "GET"))
    })
}

/// Executes a manifest-declared handler through the generated BEAM artifacts.
///
/// Inputs:
/// - `web_root`: package root, normally `_build/web`.
/// - `handler`: validated manifest handler target.
/// - `method`: request method as parsed from the HTTP request line.
/// - `request_path`: request URL path without query text.
///
/// Output:
/// - Parsed handler response when BEAM execution succeeds.
/// - Stable `error[serve_handler]` text when artifacts, `erl`, execution, or
///   handler return shape are invalid.
///
/// Transformation:
/// - Resolves the sibling `_build/ebin` directory, invokes `erl -noshell` with
///   the generated BEAM code path, passes a small request map, and parses the
///   stable `{terlan_response, Status, ContentType, Body}` ABI printed by the
///   Erlang runner expression.
pub(super) fn execute_beam_handler(
    web_root: &Path,
    handler: &WebPackageHandler,
    method: &str,
    request_path: &str,
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
            "error[serve_handler]: BEAM module `{}` for handler `{}` `{}` was not found at `{}`",
            handler.module,
            handler.method,
            handler.route,
            beam_path.display()
        ));
    }

    let request = native_http::Request::from_parts(method, request_path, "");
    let eval = render_beam_handler_eval(&erlang_module, &handler.function, &request);
    let output = Command::new("erl")
        .arg("-noshell")
        .arg("-pa")
        .arg(&ebin_dir)
        .arg("-eval")
        .arg(eval)
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
            "error[serve_handler]: handler `{}.{}/1` failed through BEAM: {detail}",
            handler.module, handler.function
        ));
    }

    parse_beam_handler_stdout(&output.stdout)
}

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
fn beam_ebin_dir_for_web_root(web_root: &Path) -> Result<PathBuf, String> {
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
/// - `method`: HTTP method text.
/// - `request_path`: URL path without query text.
///
/// Output:
/// - Erlang `-eval` source that exits zero only for the stable response ABI.
///
/// Transformation:
/// - Builds a small request map and converts the handler return value into a
///   three-part stdout protocol: status line, content-type line, and raw body.
fn render_beam_handler_eval(
    erlang_module: &str,
    function: &str,
    request: &native_http::Request,
) -> String {
    let method = erlang_binary_literal(request.method().as_bytes());
    let path = erlang_binary_literal(request.path().as_bytes());
    format!(
        "Request = #{{method => {method}, path => {path}}}, Result = catch {erlang_module}:{function}(Request), case Result of {{terlan_response, Status, ContentType, Body}} when is_integer(Status), is_binary(ContentType), is_binary(Body) -> io:format(\"~B~n~ts~n\", [Status, ContentType]), io:put_chars(Body), halt(0); {{'EXIT', Reason}} -> io:format(standard_error, \"handler failed: ~p~n\", [Reason]), halt(11); Other -> io:format(standard_error, \"handler returned unsupported value: ~p~n\", [Other]), halt(12) end."
    )
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

/// Parses stdout from the BEAM handler runner.
///
/// Inputs:
/// - `stdout`: bytes written by the Erlang runner expression.
///
/// Output:
/// - Parsed handler response or stable serve-handler error text.
///
/// Transformation:
/// - Reads the first line as status, the second line as content type, and
///   preserves all remaining bytes as the response body.
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

    let body = String::from_utf8(rest[second_newline + 1..].to_vec())
        .map_err(|err| format!("error[serve_handler]: BEAM handler body is not UTF-8: {err}"))?;
    let native_response = native_http::Response::from_parts(status as i64, content_type, body);
    BeamHandlerResponse::from_native_response(&native_response)
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
