use std::path::Path;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use terlan_http_native as native_http;
use terlan_http_native::channel_admission::{self, Admission};

use super::sse_invocation::AotSseCallbackSession;

use super::{
    finish_router_response, vm_request_descriptor, HandlerResponse, RouterResponseRuntime,
    WebPackageHandler, WebPackageSse,
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
    let outcome =
        channel_admission::sse(&router, &endpoint.route, request.path(), |middleware, _| {
            vm.execute_callable(
                &endpoint.module,
                middleware,
                vec![middleware_request.clone()],
                output,
            )
            .map_err(String::from)
        })?;
    match outcome {
        Admission::Respond(short) => finish_router_response(
            RouterResponseRuntime::new(&vm, &endpoint.module, request, package_root),
            output,
            short.response,
            short.route_params,
            short.response_middleware,
        )
        .map(VmSseRouterAdmission::Respond),
        Admission::Open(plan) => {
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
            crate::commands::serve::handler::sse_invocation::open(
                vm,
                endpoint.module.clone(),
                session,
            )
            .map(|session| VmSseRouterAdmission::Stream(Box::new(session)))
        }
    }
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
