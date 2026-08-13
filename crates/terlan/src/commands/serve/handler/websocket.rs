use std::path::Path;
use std::sync::Arc;

use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouteTarget, VmHttpRouterOutcome};
use crate::terlan_native::http as native_http;

use super::websocket_invocation::AotWebSocketCallbackSession;

use super::{
    finish_router_response, validate_handler_module, validate_handler_route, validate_source_span,
    vm_request_descriptor, HandlerResponse, RouterResponseRuntime, WebPackageHandler,
    WebPackageWebSocket,
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
    let outcome = router.dispatch_with_typed_middleware(
        VmHttpRouteMethod::Get,
        request.path(),
        |middleware, _| {
            vm.execute_callable(
                &websocket.module,
                middleware,
                vec![middleware_request.clone()],
                output,
            )
        },
    )?;
    match outcome {
        VmHttpRouterOutcome::ShortCircuited(short) => finish_router_response(
            RouterResponseRuntime::new(&vm, &websocket.module, request, package_root),
            output,
            short.response,
            short.route_params,
            short.response_middleware,
        )
        .map(VmWebSocketRouterAdmission::Respond)
        .map(Some),
        VmHttpRouterOutcome::Matched(dispatch) => {
            if dispatch.method != VmHttpRouteMethod::Get
                || dispatch.route_pattern != websocket.route
            {
                return Err(format!(
                    "error[serve_router]: websocket route `GET` `{}` does not match materialized route `{}` `{}`",
                    websocket.route,
                    dispatch.method.as_str(),
                    dispatch.route_pattern
                ));
            }
            let VmHttpRouteTarget::WebSocketEndpoint(plan) = dispatch.target else {
                return Err(format!(
                    "error[serve_router]: websocket route `GET` `{}` did not resolve to a WebSocket endpoint",
                    websocket.route
                ));
            };
            let live = crate::runtime::vm::websocket::VmWebSocketLiveSession::open(plan);
            AotWebSocketCallbackSession::open(vm, websocket.module.clone(), live)
                .map(|session| VmWebSocketRouterAdmission::Upgrade(Box::new(session)))
                .map(Some)
        }
        VmHttpRouterOutcome::NotFound => Err(format!(
            "error[serve_router]: materialized router did not match websocket GET {}",
            request.path()
        )),
    }
}

/// Validates one WebSocket manifest route and its optional source owner.
pub(in crate::commands::serve) fn validate_websocket(
    websocket: &WebPackageWebSocket,
) -> Result<(), String> {
    validate_handler_route(&websocket.route)?;
    if !websocket.module.is_empty() {
        validate_handler_module(&websocket.module)?;
        if websocket.source.is_none() {
            return Err(format!(
                "error[serve_package]: websocket `{}` router owner `{}` is missing source metadata",
                websocket.route, websocket.module
            ));
        }
    }
    if websocket.protocol.trim().is_empty() || websocket.protocol.contains(char::is_whitespace) {
        return Err(format!(
            "error[serve_package]: websocket `{}` has invalid protocol `{}`",
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
