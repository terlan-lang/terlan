use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use crate::runtime::vm::http_router::{
    validate_response_middleware_result, VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouterOutcome,
};
use crate::runtime::vm::{ReplValue, VmHttpCallResult};
use crate::terlan_native::http as native_http;
use crate::web_route::{is_identifier, route_param_names, validate_route_pattern};

use super::handler_cache::AotHandlerRuntime;
#[cfg(test)]
use super::manifest::read_web_manifest;
#[cfg(test)]
use super::package_relative_path;
use super::RELOAD_ENDPOINT;

mod channel_invocation;
#[cfg(test)]
mod manifest_lookup;
pub(super) mod request_materialization;
mod response_bridge;
mod route;
mod sse;
mod sse_invocation;
mod suspendable;
mod types;
mod websocket;
mod websocket_invocation;

/// Admitted long-lived channel retained until production socket handoff.
#[derive(Debug)]
pub(super) enum VmHttpChannelTransport {
    /// WebSocket callback and bounded inbound ownership after HTTP upgrade.
    WebSocket(websocket_invocation::AotWebSocketCallbackSession),
    /// SSE callback and bounded event ownership after HTTP admission.
    Sse(sse_invocation::AotSseCallbackSession),
}

#[cfg(test)]
pub(super) use manifest_lookup::{
    manifest_file_response_for_request, manifest_handler_for_request,
    manifest_static_response_for_request,
};
use request_materialization::vm_source_request_tuple_owned;
use response_bridge::validate_response_header;
pub(super) use response_bridge::{static_response_header_tuples, HandlerBody, HandlerResponse};
use route::route_param_argument;
#[cfg(test)]
use route::select_handler_for_request;
pub(super) use route::{
    manifest_route_for_request, validate_handler_routes, MatchedWebPackageHandler,
    MatchedWebPackageRoute,
};
pub(super) use sse::{
    execute_vm_router_sse_admission_with_package_root, sse_router_handler, validate_sse,
    VmSseRouterAdmission,
};
#[cfg(test)]
pub(in crate::commands::serve) use sse_invocation::AotSseCallbackSession;
pub(super) use suspendable::execute_suspendable_vm_handler_with_package_root_projected;
pub(super) use types::{
    WebPackageErrorHandler, WebPackageFileResponse, WebPackageHandler, WebPackageSourceSpan,
    WebPackageSse, WebPackageStaticResponse, WebPackageWebSocket,
};
pub(super) use websocket::{
    execute_vm_router_websocket_admission_with_package_root, validate_websocket,
    websocket_router_handler, VmWebSocketRouterAdmission,
};
#[cfg(test)]
pub(in crate::commands::serve) use websocket_invocation::AotWebSocketCallbackSession;

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
#[cfg(test)]
pub(super) struct HandlerLogIdentity<'a> {
    pub(super) method: &'a str,
    pub(super) route: &'a str,
    pub(super) module: &'a str,
    pub(super) function: &'a str,
    pub(super) arity: usize,
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
#[cfg(test)]
pub(super) fn handler_log_identity(matched: &MatchedWebPackageHandler) -> HandlerLogIdentity<'_> {
    HandlerLogIdentity {
        method: &matched.handler.method,
        route: &matched.handler.route,
        module: &matched.handler.module,
        function: &matched.handler.function,
        arity: matched.handler.arity,
        source: matched.handler.source.as_ref(),
    }
}

/// Executes with the exact request projection already selected by the active
/// generation, avoiding a second export metadata lookup on the hot path.
pub(super) fn execute_vm_handler_with_package_root_projected(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
) -> Result<HandlerResponse, String> {
    match execute_vm_handler_response(vm, matched, request, projection, output)? {
        VmHttpCallResult::Response(response) => HandlerResponse::from_aot_http_response(response),
        VmHttpCallResult::Generic(value) => {
            HandlerResponse::from_owned_vm_response_with_package_root(value, package_root)
        }
    }
}

/// Executes a manifest-selected request through its source router graph.
///
/// Inputs:
/// - `vm`: runtime containing the route module.
/// - `matched`: manifest route used to locate the owning module.
/// - `request`: typed native request snapshot.
/// - `package_root`: generated web package root used by file responses.
/// - `output`: sink for middleware and handler console effects.
///
/// Output:
/// - `Some(response)` when the module declares `router/0` and graph dispatch
///   completes or short-circuits.
/// - `None` for older source/package pairs without `router/0`.
/// - Stable router, middleware, callable, or response diagnostics otherwise.
///
/// Transformation:
/// - Materializes the checked `std.http.Router` descriptor, dispatches the
///   manifest-selected method/path through ordered typed middleware, and
///   invokes the resulting source handler closure through the same VM module.
pub(super) fn execute_vm_router_handler_with_package_root(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: &native_http::Request,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
) -> Result<Option<HandlerResponse>, String> {
    execute_vm_router_with_package_root(vm, matched, request, package_root, output, None)
}

/// Executes one compiler-folded response through its exact source router route.
pub(super) fn execute_vm_router_static_response_with_package_root(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    response: &WebPackageStaticResponse,
    request: &native_http::Request,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
) -> Result<Option<HandlerResponse>, String> {
    let prepared = PreparedRouterResponse {
        method: vm_route_method(&response.method)?,
        route_pattern: response.route.clone(),
        value: static_response_vm_value(response),
    };
    execute_vm_router_with_package_root(vm, matched, request, package_root, output, Some(prepared))
}

struct PreparedRouterResponse {
    method: VmHttpRouteMethod,
    route_pattern: String,
    value: ReplValue,
}

fn execute_vm_router_with_package_root(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: &native_http::Request,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
    prepared: Option<PreparedRouterResponse>,
) -> Result<Option<HandlerResponse>, String> {
    const ROUTER_FUNCTION: &str = "router";
    let module = &matched.handler.module;
    if !vm.has_function(module, ROUTER_FUNCTION, 0) {
        return Ok(None);
    }

    let router = vm.execute_http_router(module, ROUTER_FUNCTION, output)?;
    let method = vm_route_method(&matched.handler.method)?;
    let middleware_request = vm_request_descriptor(request, &matched.params);
    let outcome =
        match router.dispatch_with_typed_middleware(method, request.path(), |middleware, _| {
            vm.execute_callable(module, middleware, vec![middleware_request.clone()], output)
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                let response = execute_router_recovery(vm, module, &router, error, output)?;
                return finish_router_response(
                    RouterResponseRuntime::new(vm, module, request, package_root),
                    output,
                    response,
                    Vec::new(),
                    Vec::new(),
                )
                .map(Some);
            }
        };
    let (response, route_params, response_middleware) = match outcome {
        VmHttpRouterOutcome::ShortCircuited(short) => (
            short.response,
            short.route_params,
            short.response_middleware,
        ),
        VmHttpRouterOutcome::Matched(dispatch) => {
            let route_params = dispatch.route_params.clone();
            let response_middleware = dispatch.response_middleware.clone();
            if prepared.is_none()
                && (dispatch.method != method || dispatch.route_pattern != matched.handler.route)
            {
                return Err(format!(
                    "error[serve_router]: manifest route `{}` `{}` does not match materialized route `{}` `{}`",
                    matched.handler.method,
                    matched.handler.route,
                    dispatch.method.as_str(),
                    dispatch.route_pattern
                ));
            }
            let VmHttpRouteTarget::Handler(handler) = dispatch.target else {
                return Err(format!(
                    "error[serve_router]: route {} {} did not resolve to a source handler",
                    method.as_str(),
                    dispatch.path
                ));
            };
            let arity = vm.callable_arity(&handler).ok_or_else(|| {
                "error[serve_router]: matched route target is not callable".to_string()
            })?;
            if let Some(prepared) = &prepared {
                if dispatch.method != prepared.method
                    || dispatch.route_pattern != prepared.route_pattern
                {
                    return Err(format!(
                        "error[serve_router]: folded static route `{}` `{}` does not match materialized route `{}` `{}`",
                        prepared.method.as_str(),
                        prepared.route_pattern,
                        dispatch.method.as_str(),
                        dispatch.route_pattern
                    ));
                }
                return finish_router_response_with_recovery(
                    RouterResponseRuntime::new(vm, module, request, package_root),
                    &router,
                    output,
                    prepared.value.clone(),
                    route_params,
                    response_middleware,
                )
                .map(Some);
            }
            let mut args = vec![vm_request_descriptor(request, &dispatch.route_params)];
            if arity > 1 {
                args.extend(
                    dispatch
                        .route_params
                        .iter()
                        .map(|(name, value)| {
                            route_param_argument(&dispatch.route_pattern, name, value)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            if args.len() != arity {
                return Err(format!(
                    "error[serve_router]: route {} {} handler expects {arity} argument(s), found {}",
                    method.as_str(),
                    dispatch.path,
                    args.len()
                ));
            }
            let response = match vm.execute_callable(module, &handler, args, output) {
                Ok(response) => response,
                Err(error) => execute_router_recovery(vm, module, &router, error, output)?,
            };
            (response, route_params, response_middleware)
        }
        VmHttpRouterOutcome::NotFound => {
            return Err(format!(
                "error[serve_router]: materialized router did not match {} {}",
                method.as_str(),
                request.path()
            ));
        }
    };
    finish_router_response_with_recovery(
        RouterResponseRuntime::new(vm, module, request, package_root),
        &router,
        output,
        response,
        route_params,
        response_middleware,
    )
    .map(Some)
}

#[derive(Clone, Copy)]
pub(super) struct RouterResponseRuntime<'a> {
    vm: &'a AotHandlerRuntime,
    module: &'a str,
    request: &'a native_http::Request,
    package_root: &'a Path,
}

impl<'a> RouterResponseRuntime<'a> {
    pub(super) fn new(
        vm: &'a AotHandlerRuntime,
        module: &'a str,
        request: &'a native_http::Request,
        package_root: &'a Path,
    ) -> Self {
        Self {
            vm,
            module,
            request,
            package_root,
        }
    }
}

fn finish_router_response_with_recovery(
    runtime: RouterResponseRuntime<'_>,
    router: &crate::runtime::vm::http_router::VmHttpRouter,
    output: &mut dyn FnMut(&str),
    response: ReplValue,
    route_params: Vec<(String, String)>,
    response_middleware: Vec<ReplValue>,
) -> Result<HandlerResponse, String> {
    match finish_router_response(
        runtime,
        output,
        response,
        route_params.clone(),
        response_middleware,
    ) {
        Ok(response) => Ok(response),
        Err(error) => {
            let recovered =
                execute_router_recovery(runtime.vm, runtime.module, router, error, output)?;
            finish_router_response(runtime, output, recovered, route_params, Vec::new())
        }
    }
}

pub(super) fn execute_router_recovery(
    vm: &AotHandlerRuntime,
    module: &str,
    router: &crate::runtime::vm::http_router::VmHttpRouter,
    error: String,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let Some(handler) = router.error_handler() else {
        return Err(error);
    };
    let http_error = ReplValue::Record {
        name: "HttpError".to_string(),
        fields: vec![
            (
                "code".to_string(),
                ReplValue::Atom("router_execution_failed".to_string()),
            ),
            ("message".to_string(), ReplValue::String(error.clone())),
            ("status".to_string(), ReplValue::Int(500)),
        ],
    };
    vm.execute_callable(module, handler, vec![http_error], output)
        .map_err(|recovery| {
            format!(
                "error[serve_router_recovery]: router failed with `{error}`; error handler failed with `{recovery}`"
            )
        })
}

fn finish_router_response(
    runtime: RouterResponseRuntime<'_>,
    output: &mut dyn FnMut(&str),
    mut response: ReplValue,
    route_params: Vec<(String, String)>,
    response_middleware: Vec<ReplValue>,
) -> Result<HandlerResponse, String> {
    let response_request = vm_request_descriptor(runtime.request, &route_params);
    for middleware in response_middleware.iter().rev() {
        response = runtime.vm.execute_callable(
            runtime.module,
            middleware,
            vec![response_request.clone(), response],
            output,
        )?;
        validate_response_middleware_result(&response)?;
    }
    HandlerResponse::from_vm_response_with_package_root(&response, runtime.package_root)
}

fn static_response_vm_value(response: &WebPackageStaticResponse) -> ReplValue {
    let kind = if response.content_type == "text/html; charset=utf-8" {
        1
    } else {
        0
    };
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::Int(kind),
        ReplValue::String(response.body.clone()),
        ReplValue::Int(i64::from(response.status)),
        ReplValue::String(String::new()),
        ReplValue::List(
            response
                .headers
                .iter()
                .map(|header| {
                    ReplValue::Tuple(vec![
                        ReplValue::String(header.name.clone()),
                        ReplValue::String(header.value.clone()),
                    ])
                })
                .collect(),
        ),
    ])
}

/// Converts a validated manifest method into the VM router method domain.
fn vm_route_method(method: &str) -> Result<VmHttpRouteMethod, String> {
    VmHttpRouteMethod::from_name(method)
        .ok_or_else(|| format!("error[serve_router]: unsupported router method `{method}`"))
}

/// Executes one VM handler with direct managed-Response extraction when possible.
fn execute_vm_handler_response(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    output: &mut dyn FnMut(&str),
) -> Result<VmHttpCallResult, String> {
    if matched.handler.arity == 1 {
        return vm.execute_projected_http_request(
            &matched.handler.module,
            &matched.handler.function,
            request.into_parts(),
            projection,
            output,
        );
    }
    let request = vm_source_request_tuple_owned(request.into_parts());
    let mut args = vec![request];
    if matched.handler.arity > 1 {
        args.extend(
            matched
                .params
                .iter()
                .map(|(name, value)| route_param_argument(&matched.handler.route, name, value))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if args.len() != matched.handler.arity {
        return Err(format!(
            "error[serve_handler]: handler `{}.{}/{}` received {} VM argument(s)",
            matched.handler.module,
            matched.handler.function,
            matched.handler.arity,
            args.len()
        ));
    }
    vm.execute_immediate_http_response(
        &matched.handler.module,
        &matched.handler.function,
        args,
        output,
    )
}

/// Builds a borrowed request descriptor for router and long-lived channels.
fn vm_request_descriptor(request: &native_http::Request, params: &[(String, String)]) -> ReplValue {
    let cookies = string_map(request.cookie_pairs());
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::String(request.method().to_string()),
        ReplValue::String(request.path().to_string()),
        string_map(params),
        ReplValue::String(request.body().to_string()),
        ReplValue::String(request.query_string().to_string()),
        string_map(request.query_pairs()),
        string_map(request.header_pairs()),
        cookies.clone(),
        ReplValue::Tuple(vec![cookies, ReplValue::List(Vec::new())]),
        ReplValue::String(request.body_file_path().to_string()),
    ])
}

/// Builds a VM string map from request metadata pairs.
fn string_map(entries: &[(String, String)]) -> ReplValue {
    ReplValue::Map(
        entries
            .iter()
            .map(|(key, value)| {
                (
                    ReplValue::String(key.clone()),
                    ReplValue::String(value.clone()),
                )
            })
            .collect(),
    )
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
    validate_static_response_owner(response)?;
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

fn validate_static_response_owner(response: &WebPackageStaticResponse) -> Result<(), String> {
    let owner_parts = [
        !response.module.trim().is_empty(),
        !response.function.trim().is_empty(),
        response.arity > 0,
    ];
    if owner_parts.iter().any(|present| *present) && !owner_parts.iter().all(|present| *present) {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` has incomplete router owner metadata",
            response.method, response.route
        ));
    }
    if owner_parts.iter().all(|present| *present) && response.arity != 1 {
        return Err(format!(
            "error[serve_package]: static response `{}` `{}` router handler must have arity 1",
            response.method, response.route
        ));
    }
    Ok(())
}

/// Projects a compiler-folded static response back to its source router owner.
pub(super) fn static_response_router_handler(
    response: &WebPackageStaticResponse,
) -> Option<WebPackageHandler> {
    if response.module.is_empty()
        || response.function.is_empty()
        || response.arity == 0
        || response.source.is_none()
    {
        return None;
    }
    Some(WebPackageHandler {
        method: response.method.clone(),
        route: response.route.clone(),
        module: response.module.clone(),
        function: response.function.clone(),
        arity: response.arity,
        source: response.source.clone(),
    })
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
    if VmHttpRouteMethod::from_name(method).is_some() {
        Ok(())
    } else {
        Err(format!(
            "error[serve_package]: unsupported handler method `{method}`"
        ))
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
        validate_route_pattern(route)?;
        return Ok(());
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
