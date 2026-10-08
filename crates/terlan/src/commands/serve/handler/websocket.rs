use std::path::Path;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use terlan_http_native as native_http;
use terlan_http_native::channel_admission::{self, Admission};

use super::websocket_invocation::AotWebSocketCallbackSession;

use super::{
    finish_router_response, vm_request_descriptor, HandlerResponse, RouterResponseRuntime,
    WebPackageHandler, WebPackageWebSocket,
};

/// Result of source-router admission for one WebSocket upgrade.
#[derive(Debug)]
pub(in crate::commands::serve) enum VmWebSocketRouterAdmission {
    /// The materialized graph selected a WebSocket endpoint for the route.
    Upgrade(Box<AotWebSocketCallbackSession>),
    /// Typed middleware terminated the request with a normal HTTP response.
    Respond(HandlerResponse),
}

/// Executes WebSocket upgrade admission through its source router graph.
pub(in crate::commands::serve) fn execute_vm_router_websocket_admission_with_package_root(
    vm: Arc<AotHandlerRuntime>,
    websocket: &WebPackageWebSocket,
    request: &native_http::Request,
    package_root: &Path,
    output: &mut dyn FnMut(&str),
) -> Result<Option<VmWebSocketRouterAdmission>, String> {
    const ROUTER_FUNCTION: &str = "router";
    if !vm.has_function(&websocket.module, ROUTER_FUNCTION, 0) {
        return Ok(None);
    }

    let router = vm.execute_http_router(&websocket.module, ROUTER_FUNCTION, output)?;
    let middleware_request = vm_request_descriptor(request, &[]);
    let outcome = channel_admission::websocket(
        &router,
        &websocket.route,
        request.path(),
        |middleware, _| {
            vm.execute_callable(
                &websocket.module,
                middleware,
                vec![middleware_request.clone()],
                output,
            )
            .map_err(String::from)
        },
    )?;
    match outcome {
        Admission::Respond(short) => finish_router_response(
            RouterResponseRuntime::new(&vm, &websocket.module, request, package_root),
            output,
            short.response,
            short.route_params,
            short.response_middleware,
        )
        .map(VmWebSocketRouterAdmission::Respond)
        .map(Some),
        Admission::Open(plan) => {
            let live = terlan_http_native::websocket::session::Session::open(plan);
            crate::commands::serve::handler::websocket_invocation::open(
                vm,
                websocket.module.clone(),
                live,
            )
            .map(|session| VmWebSocketRouterAdmission::Upgrade(Box::new(session)))
            .map(Some)
        }
    }
}

/// Projects a source-owned WebSocket route into the shared VM module loader.
pub(in crate::commands::serve) fn websocket_router_handler(
    websocket: &WebPackageWebSocket,
) -> Option<WebPackageHandler> {
    if websocket.module.is_empty() || websocket.source.is_none() {
        return None;
    }
    Some(WebPackageHandler {
        method: "GET".to_string(),
        route: websocket.route.clone(),
        module: websocket.module.clone(),
        function: "router".to_string(),
        arity: 0,
        source: websocket.source.clone(),
    })
}
