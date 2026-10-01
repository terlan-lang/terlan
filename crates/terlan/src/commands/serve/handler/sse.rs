use std::path::Path;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouterOutcome};
use crate::terlan_native::http as native_http;

use super::sse_invocation::AotSseCallbackSession;

use super::{
    finish_router_response, validate_handler_module, validate_handler_route, validate_source_span,
    vm_request_descriptor, HandlerResponse, RouterResponseRuntime, WebPackageHandler,
    WebPackageSse,
};

/// Result of source-router admission for one SSE request.
#[derive(Debug)]
pub(in crate::commands::serve) enum VmSseRouterAdmission {
    Stream(Box<AotSseCallbackSession>),
    Respond(HandlerResponse),
}

/// Executes SSE admission through the materialized source router graph.
pub(in crate::commands::serve) fn execute_vm_router_sse_admission_with_package_root(
    vm: Arc<AotHandlerRuntime>,
    endpoint: &WebPackageSse,
    request: &native_http::Request,
    package_root: &Path,
    live_transport_available: bool,
    output: &mut dyn FnMut(&str),
) -> Result<VmSseRouterAdmission, String> {
    let router = vm.execute_http_router(&endpoint.module, "router", output)?;
    let middleware_request = vm_request_descriptor(request, &[]);
    let outcome = router.dispatch_with_typed_middleware(
        VmHttpRouteMethod::Get,
        request.path(),
        |middleware, _| {
            vm.execute_callable(
                &endpoint.module,
                middleware,
                vec![middleware_request.clone()],
                output,
            )
            .map_err(String::from)
        },
    )?;
    match outcome {
        VmHttpRouterOutcome::ShortCircuited(short) => finish_router_response(
            RouterResponseRuntime::new(&vm, &endpoint.module, request, package_root),
            output,
            short.response,
            short.route_params,
            short.response_middleware,
        )
        .map(VmSseRouterAdmission::Respond),
        VmHttpRouterOutcome::Matched(dispatch) => {
            if dispatch.method != VmHttpRouteMethod::Get || dispatch.route_pattern != endpoint.route
            {
                return Err(format!(
                    "error[serve_router]: SSE route `GET` `{}` does not match materialized route `{}` `{}`",
                    endpoint.route,
                    dispatch.method.as_str(),
                    dispatch.route_pattern
                ));
            }
            let VmHttpRouteTarget::SseEndpoint(plan) = dispatch.target else {
                return Err(format!(
                    "error[serve_router]: SSE route `GET` `{}` did not resolve to an SSE endpoint",
                    endpoint.route
                ));
            };
            // Middleware may return a normal response even without a live
            // transport. Never run open callbacks for a stream we cannot serve.
            if !live_transport_available {
                return Ok(VmSseRouterAdmission::Respond(HandlerResponse {
                    status: 501,
                    content_type: "text/plain; charset=utf-8".into(),
                    headers: Vec::new(),
                    body: super::HandlerBody::Text(
                        "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for SSE".into(),
                    ),
                }));
            }
            let session = terlan_http_native::sse_session::SseSession::open(plan);
            AotSseCallbackSession::open(vm, endpoint.module.clone(), session)
                .map(|session| VmSseRouterAdmission::Stream(Box::new(session)))
        }
        VmHttpRouterOutcome::NotFound => Err(format!(
            "error[serve_router]: materialized router did not match SSE GET {}",
            request.path()
        )),
    }
}

/// Validates one source-owned SSE manifest route.
pub(in crate::commands::serve) fn validate_sse(endpoint: &WebPackageSse) -> Result<(), String> {
    validate_handler_route(&endpoint.route)?;
    validate_handler_module(&endpoint.module)?;
    validate_source_span("SSE", &format!("GET {}", endpoint.route), &endpoint.source)
}

/// Projects an SSE route into the shared source-module loader.
pub(in crate::commands::serve) fn sse_router_handler(
    endpoint: &WebPackageSse,
) -> WebPackageHandler {
    WebPackageHandler {
        method: "GET".to_string(),
        route: endpoint.route.clone(),
        module: endpoint.module.clone(),
        function: "router".to_string(),
        arity: 0,
        source: Some(endpoint.source.clone()),
    }
}
