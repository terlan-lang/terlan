use std::path::Path;

use crate::runtime::vm::ReplValue;
use terlan_http_native as native_http;
use terlan_http_native::routing::RouteMethod;

use super::handler_cache::AotHandlerRuntime;

mod channel_invocation;
pub(super) mod request_materialization;
mod response_bridge;
mod route;
mod router_host;
use router_host::RouterHost;
mod sse;
mod sse_invocation;
mod suspendable;
mod suspendable_router;
pub(super) use suspendable_router::execute_suspendable_router;
mod suspendable_channel;
pub(super) use suspendable_channel::{execute_suspendable_sse, execute_suspendable_websocket};
mod manifest_validation;
mod types;
pub(super) use manifest_validation::{
    validate_error_handler, validate_file_response, validate_handler, validate_sse,
    validate_static_response, validate_websocket,
};
#[cfg(test)]
mod protocol_test_support;
mod websocket;
mod websocket_invocation;

/// Admitted long-lived channel retained until production socket handoff.
pub(super) type VmHttpChannelTransport = terlan_http_native::request_pipeline::Channel<
    websocket_invocation::AotWebSocketCallbackSession,
    sse_invocation::AotSseCallbackSession,
>;

use request_materialization::direct_handler_arguments;
#[cfg(test)]
pub(super) use response_bridge::decode_response;
use response_bridge::static_response_vm_value;
pub(super) use response_bridge::{
    decode_owned_response, static_response_header_tuples, HandlerResponse,
};
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

type PreparedRouterResponse = native_http::router_execution::PreparedResponse<ReplValue>;

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
    native_http::router_execution::execute(
        &router,
        method,
        &matched.handler.route,
        request.path(),
        &matched.params,
        prepared,
        &mut RouterHost {
            runtime: RouterResponseRuntime::new(vm, module, request, package_root),
            output,
        },
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

#[cfg(test)]
pub(super) fn execute_router_recovery(
    vm: &AotHandlerRuntime,
    module: &str,
    router: &terlan_http_native::routing::Router<ReplValue>,
    error: String,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    native_http::router_execution::recover(
        router,
        error,
        |callable, args| {
            vm.execute_callable(module, callable, args, output)
                .map_err(String::from)
        },
        crate::runtime::vm::native_value::from_native,
    )
}

fn finish_router_response(
    runtime: RouterResponseRuntime<'_>,
    output: &mut dyn FnMut(&str),
    response: ReplValue,
    route_params: Vec<(String, String)>,
    response_middleware: Vec<ReplValue>,
) -> Result<HandlerResponse, String> {
    native_http::router_execution::finish(
        response,
        &route_params,
        &response_middleware,
        &mut RouterHost { runtime, output },
    )
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

#[cfg(test)]
mod source_request_assertions_test;
