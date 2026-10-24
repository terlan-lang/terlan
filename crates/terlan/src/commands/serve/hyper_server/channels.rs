//! Pins source images while the HTTP package executes channel admission.

use super::super::handler::{
    self, MatchedWebPackageRoute, VmSseRouterAdmission, VmWebSocketRouterAdmission,
};
use super::super::handler_cache::cached_vm_handler_runtime_for_request;
use super::super::response_rendering::serve_vm_stream_handler_response;
use super::*;

pub(super) async fn handle(
    web_root: &Path,
    request: &Request<String>,
    channel: &mut Option<VmHttpChannelTransport>,
) -> Result<Option<Response<Bytes>>, String> {
    match handler::manifest_route_for_request(
        web_root,
        request.method().as_str(),
        request.uri().path(),
    ) {
        Some(MatchedWebPackageRoute::Sse(endpoint)) => {
            sse(web_root, &endpoint, request, channel).await.map(Some)
        }
        Some(MatchedWebPackageRoute::WebSocket(endpoint)) => {
            websocket(web_root, &endpoint, request, channel)
                .await
                .map(Some)
        }
        _ => Ok(None),
    }
}

async fn sse(
    web_root: &Path,
    endpoint: &handler::WebPackageSse,
    request: &Request<String>,
    channel: &mut Option<VmHttpChannelTransport>,
) -> Result<Response<Bytes>, String> {
    if let Some(response) = super::super::request_dispatch::reserved_stream_response(
        web_root,
        request.method().as_str(),
        request.uri().path(),
    )? {
        return Ok(response);
    }
    let runtime =
        cached_vm_handler_runtime_for_request(web_root, &handler::sse_router_handler(endpoint))?;
    let response = match handler::execute_suspendable_sse(
        runtime.into_runtime(),
        endpoint,
        &project_request(request),
        web_root,
    )
    .await?
    {
        VmSseRouterAdmission::Respond(response) => response,
        VmSseRouterAdmission::Stream(session) => {
            let response =
                terlan_http_native::channel_admission::sse_stream_response(session.plan());
            *channel = Some(VmHttpChannelTransport::Sse(*session));
            response
        }
    };
    serve_vm_stream_handler_response(response, request.method() == http::Method::HEAD)
}

async fn websocket(
    web_root: &Path,
    endpoint: &handler::WebPackageWebSocket,
    request: &Request<String>,
    channel: &mut Option<VmHttpChannelTransport>,
) -> Result<Response<Bytes>, String> {
    use terlan_http_native::websocket::handshake::{opening_handshake, OpeningHandshake};
    let upgrade = match opening_handshake(request.method(), request.version(), request.headers()) {
        OpeningHandshake::Reject(response) => return Ok(response),
        OpeningHandshake::Upgrade(response) => response,
    };
    let Some(owner) = handler::websocket_router_handler(endpoint) else {
        return Ok(upgrade);
    };
    let runtime = cached_vm_handler_runtime_for_request(web_root, &owner)?;
    match handler::execute_suspendable_websocket(
        runtime.into_runtime(),
        endpoint,
        &project_request(request),
        web_root,
    )
    .await?
    {
        Some(VmWebSocketRouterAdmission::Respond(response)) => {
            serve_vm_stream_handler_response(response, false)
        }
        Some(VmWebSocketRouterAdmission::Upgrade(session)) => {
            *channel = Some(VmHttpChannelTransport::WebSocket(*session));
            Ok(upgrade)
        }
        None => Ok(upgrade),
    }
}

fn project_request(request: &Request<String>) -> terlan_http_native::Request {
    terlan_http_native::Request::from_http(
        request,
        &[],
        terlan_http_native::RequestFieldProjection::Complete,
    )
}
