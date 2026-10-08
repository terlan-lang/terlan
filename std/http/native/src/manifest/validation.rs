//! Structural admission independent of host filesystems and VM values.

use crate::ServiceError;

use super::*;
use crate::route_pattern::{is_identifier, route_param_names};
use crate::routing::RouteMethod;
use std::path::Path;

/// Validates one dynamic HTTP handler manifest entry.
///
/// Inputs:
/// - `handler`: manifest-declared route and Terlan function target.
///
/// Output:
/// - `Ok(())` when the handler entry is safe and supported.
/// - `Err(ServiceError)` with a stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Checks route shape, allowed HTTP method, module/function spelling, and
///   handler arity before the server reserves the route.
pub fn validate_handler(handler: &HandlerRoute) -> Result<(), ServiceError> {
    validate_handler_method(&handler.method)?;
    let route_param_count = validate_handler_route(&handler.route)?;
    validate_handler_module(&handler.module)?;
    validate_handler_function(&handler.function)?;
    if let Some(source) = &handler.source {
        validate_source_span(
            "handler",
            &format!("{}.{}", handler.module, handler.function),
            source,
        )?;
    }
    let expected_with_params = 1 + route_param_count;
    if handler.arity != 1 && handler.arity != expected_with_params {
        return Err(format!(
            "error[serve_package]: handler `{}` `{}` must have arity 1 for Request input or arity {} for Request plus route parameter(s), got {}",
            handler.method, handler.route, expected_with_params, handler.arity
        ).into());
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
    source: &SourceSpan,
) -> Result<(), ServiceError> {
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
        )
        .into());
    }
    if source.line == 0 || source.column == 0 {
        return Err(format!(
            "error[serve_package]: {kind} `{identity}` source span must use one-based line and column"
        ).into());
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
/// - `Err(ServiceError)` with a stable serve-package diagnostic otherwise.
///
/// Transformation:
/// - Reuses module/function spelling checks from normal route handlers while
///   enforcing the single `HttpError` input expected by `std.http.Router.error`.
pub fn validate_error_handler(handler: &ErrorHandler) -> Result<(), ServiceError> {
    validate_handler_module(&handler.module)?;
    validate_handler_function(&handler.function)?;
    if handler.arity != 1 {
        return Err(format!(
            "error[serve_package]: error handler `{}.{}` must have arity 1 for HttpError input, got {}",
            handler.module, handler.function, handler.arity
        ).into());
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
pub fn validate_static_response(response: &StaticResponse) -> Result<(), ServiceError> {
    validate_handler_method(&response.method)?;
    validate_handler_route(&response.route)?;
    validate_static_response_owner(response)?;
    if !(100..=599).contains(&response.status) {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` has invalid status `{}`",
            response.method, response.route, response.status
        )
        .into());
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
        )
        .into());
    }
    for header in &response.headers {
        crate::validate_response_header(&header.name, &header.value).map_err(|error| {
            let message = format!("error[serve_handler]: {}", error.message());
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

fn validate_static_response_owner(response: &StaticResponse) -> Result<(), ServiceError> {
    let owner_parts = [
        !response.module.trim().is_empty(),
        !response.function.trim().is_empty(),
        response.arity > 0,
    ];
    if owner_parts.iter().any(|present| *present) && !owner_parts.iter().all(|present| *present) {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` has incomplete router owner metadata",
            response.method, response.route
        )
        .into());
    }
    if owner_parts.iter().all(|present| *present) && response.arity != 1 {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` router handler must have arity 1",
            response.method, response.route
        )
        .into());
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
pub fn validate_file_response(response: &FileResponse) -> Result<(), ServiceError> {
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
        )
        .into());
    }
    if !(100..=599).contains(&response.status) {
        return Err(format!(
            "error[serve_package]: file response `{}` `{}` has invalid status `{}`",
            response.method, response.route, response.status
        )
        .into());
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
            )
            .into());
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
/// - `Err(ServiceError)` for unsupported methods.
///
/// Transformation:
/// - Restricts dynamic handler declarations to the HTTP methods generated by
///   `std.http.Router` manifest extraction.
fn validate_handler_method(method: &str) -> Result<(), ServiceError> {
    if RouteMethod::from_name(method).is_some() {
        Ok(())
    } else {
        Err(format!("error[serve_package]: unsupported handler method `{method}`").into())
    }
}

/// Validates a handler route path.
///
/// Inputs:
/// - `route`: manifest-declared URL path.
///
/// Output:
/// - Capture count for safe absolute paths and the canonical `*` fallback.
/// - `Err(ServiceError)` for traversal, query strings, or fragments.
///
/// Transformation:
/// - Applies URL-route safety checks separate from filesystem path handling so
///   dynamic routes cannot escape into package file lookup semantics.
fn validate_handler_route(route: &str) -> Result<usize, ServiceError> {
    if route != "*" && (!route.starts_with('/') || route.contains('\\') || route.contains('\0')) {
        return Err(format!("error[serve_package]: unsafe handler route `{route}`").into());
    }
    if route.contains('?') || route.contains('#') {
        return Err(format!(
            "error[serve_package]: handler route `{route}` must not contain query or fragment text"
        )
        .into());
    }
    Ok(route_param_names(route).map_err(String::from)?.len())
}

/// Validates a Terlan module path in a handler target.
///
/// Inputs:
/// - `module`: manifest-declared Terlan module path.
///
/// Output:
/// - `Ok(())` when each dot-separated segment is a Terlan-style identifier.
/// - `Err(ServiceError)` otherwise.
///
/// Transformation:
/// - Performs a small lexical validation so malformed manifests fail before
///   runtime dispatch tries to resolve a module.
fn validate_handler_module(module: &str) -> Result<(), ServiceError> {
    if module
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifier(segment))
    {
        Ok(())
    } else {
        Err(format!("error[serve_package]: invalid handler module `{module}`").into())
    }
}

/// Validates a Terlan function name in a handler target.
///
/// Inputs:
/// - `function`: manifest-declared Terlan function name.
///
/// Output:
/// - `Ok(())` for a lowercase identifier.
/// - `Err(ServiceError)` otherwise.
///
/// Transformation:
/// - Keeps handler dispatch targets aligned with Terlan function naming.
fn validate_handler_function(function: &str) -> Result<(), ServiceError> {
    if is_identifier(function)
        && function
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
    {
        Ok(())
    } else {
        Err(format!("error[serve_package]: invalid handler function `{function}`").into())
    }
}

/// Validates one WebSocket manifest route and its optional source owner.
pub fn validate_websocket(websocket: &WebSocketRoute) -> Result<(), ServiceError> {
    validate_handler_route(&websocket.route)?;
    if !websocket.module.is_empty() {
        validate_handler_module(&websocket.module)?;
        if websocket.source.is_none() {
            return Err(format!(
                "error[serve_package]: websocket `{}` router owner `{}` is missing source metadata",
                websocket.route, websocket.module
            )
            .into());
        }
    }
    if websocket.protocol.trim().is_empty() || websocket.protocol.contains(char::is_whitespace) {
        return Err(format!(
            "error[serve_package]: websocket `{}` has invalid protocol `{}`",
            websocket.route, websocket.protocol
        )
        .into());
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

/// Validates one source-owned SSE manifest route.
pub fn validate_sse(endpoint: &SseRoute) -> Result<(), ServiceError> {
    validate_handler_route(&endpoint.route)?;
    validate_handler_module(&endpoint.module)?;
    validate_source_span("SSE", &format!("GET {}", endpoint.route), &endpoint.source)
}
