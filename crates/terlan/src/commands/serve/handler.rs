use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use crate::runtime::vm::ReplValue;
use terlan_http_native as native_http;
use terlan_http_native::routing::{
    validate_response_middleware_result, RouteMethod, RouteTarget, RouterOutcome,
};

use super::handler_cache::AotHandlerRuntime;
#[cfg(test)]
use super::manifest::read_web_manifest;
#[cfg(test)]
use super::package_relative_path;

mod channel_invocation;
#[cfg(test)]
mod manifest_lookup;
pub(super) mod request_materialization;
mod response_bridge;
mod route;
mod sse;
mod sse_invocation;
mod suspendable;
mod suspendable_router;
pub(super) use suspendable_router::execute_suspendable_router;
mod manifest_validation;
mod types;
pub(super) use manifest_validation::{
    validate_error_handler, validate_file_response, validate_handler, validate_sse,
    validate_static_response, validate_websocket,
};
mod websocket;
mod websocket_invocation;

/// Admitted long-lived channel retained until production socket handoff.
pub(super) type VmHttpChannelTransport = terlan_http_native::request_pipeline::Channel<
    websocket_invocation::AotWebSocketCallbackSession,
    sse_invocation::AotSseCallbackSession,
>;

#[cfg(test)]
pub(super) use manifest_lookup::{
    manifest_file_response_for_request, manifest_handler_for_request,
    manifest_static_response_for_request,
};
use request_materialization::direct_handler_arguments;
use response_bridge::static_response_vm_value;
pub(super) use response_bridge::{
    decode_owned_response, decode_response, static_response_header_tuples, HandlerBody,
    HandlerResponse,
};
use route::route_param_argument;
#[cfg(test)]
use route::select_handler_for_request;
pub(super) use route::{
    manifest_route_for_request, MatchedWebPackageHandler, MatchedWebPackageRoute,
};
pub(super) use sse::{
    execute_vm_router_sse_admission_with_package_root, sse_router_handler, VmSseRouterAdmission,
};
pub(in crate::commands::serve) use sse_invocation::AotSseCallbackSession;
pub(super) use suspendable::execute_suspendable_vm_handler_with_package_root_projected;
#[cfg(test)]
pub(super) use types::WebPackageSourceSpan;
pub(super) use types::{
    WebPackageErrorHandler, WebPackageFileResponse, WebPackageHandler, WebPackageSse,
    WebPackageStaticResponse, WebPackageWebSocket,
};
pub(super) use websocket::{
    execute_vm_router_websocket_admission_with_package_root, websocket_router_handler,
    VmWebSocketRouterAdmission,
};
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
/// generation, without recomputing field observations.
pub(super) fn execute_vm_handler_with_package_root_projected(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
) -> Result<HandlerResponse, String> {
    let value = execute_vm_handler_response(vm, matched, request, projection, output)?;
    crate::commands::serve::handler::decode_owned_response(value, package_root)
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
    method: RouteMethod,
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
                .map_err(String::from)
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                let response = execute_router_recovery(vm, module, &router, error.into(), output)?;
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
        RouterOutcome::ShortCircuited(short) => (
            short.response,
            short.route_params,
            short.response_middleware,
        ),
        RouterOutcome::Matched(dispatch) => {
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
            let RouteTarget::Handler(handler) = dispatch.target else {
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
                Err(error) => execute_router_recovery(vm, module, &router, error.into(), output)?,
            };
            (response, route_params, response_middleware)
        }
        RouterOutcome::NotFound => {
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
    router: &terlan_http_native::routing::Router<ReplValue>,
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
    router: &terlan_http_native::routing::Router<ReplValue>,
    error: String,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let Some(handler) = router.error_handler() else {
        return Err(error);
    };
    vm.execute_callable(module, handler, vec![ReplValue::String(error.clone())], output)
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
    crate::commands::serve::handler::decode_response(&response, runtime.package_root)
}

/// Converts a validated manifest method into the VM router method domain.
fn vm_route_method(method: &str) -> Result<RouteMethod, String> {
    RouteMethod::from_name(method)
        .ok_or_else(|| format!("error[serve_router]: unsupported router method `{method}`"))
}

/// Executes one VM handler with direct managed-Response extraction when possible.
fn execute_vm_handler_response(
    vm: &AotHandlerRuntime,
    matched: &MatchedWebPackageHandler,
    request: native_http::Request,
    projection: native_http::RequestFieldProjection,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    if matched.handler.arity == 1
        && vm.uses_source_request(&matched.handler.module, &matched.handler.function, 1)
    {
        return vm.execute_projected_http_request(
            &matched.handler.module,
            &matched.handler.function,
            request.into_parts(),
            projection,
            output,
        );
    }
    let args = direct_handler_arguments(vm, matched, request.into_parts(), projection)?;
    vm.execute_immediate_http_response(
        &matched.handler.module,
        &matched.handler.function,
        args,
        output,
    )
}

/// Builds a borrowed request descriptor for router and long-lived channels.
fn vm_request_descriptor(request: &native_http::Request, params: &[(String, String)]) -> ReplValue {
    let mut parts = request.clone().into_parts();
    parts.params = params.to_vec();
    request_materialization::vm_request_descriptor_owned(
        parts,
        native_http::RequestFieldProjection::Complete,
    )
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
