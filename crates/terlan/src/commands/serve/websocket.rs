use std::path::Path;

#[cfg(test)]
use bytes::Bytes;
#[cfg(test)]
use http_body_util::{BodyExt, Full};
#[cfg(test)]
use hyper::{Request, Response};
#[cfg(test)]
use std::sync::Arc;

use super::handler::WebPackageWebSocket;
#[cfg(test)]
use super::ServeBody;

/// Placeholder WebSocket runtime handle for one local `terlc serve` runtime.
///
/// Inputs:
/// - Created once per bound server and cloned through request handling.
///
/// Output:
/// - Shared marker used while WebSocket stream dispatch remains behind the
///   generic VM handler boundary.
///
/// Transformation:
/// - Keeps server setup stable without embedding application-specific socket
///   state in the Terlan command layer.
#[cfg(test)]
pub(super) type WebSocketHub = Arc<()>;

/// Builds a fresh WebSocket hub for one serve runtime.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Shared placeholder handle.
///
/// Transformation:
/// - Avoids creating application-owned room state in the compiler repository.
#[cfg(test)]
pub(super) fn websocket_hub() -> WebSocketHub {
    Arc::new(())
}

/// Finds a manifest WebSocket route for a request path.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `request_path`: URL path without query text.
///
/// Output:
/// - Matching WebSocket manifest entry, if any.
///
/// Transformation:
/// - Keeps WebSocket route discovery manifest-owned while runtime socket
///   dispatch remains blocked until the generic VM handler ABI exists.
#[allow(dead_code)] // Retained for the legacy request adapter during Hyper promotion.
pub(super) fn manifest_websocket_for_path(
    web_root: &Path,
    request_path: &str,
) -> Option<WebPackageWebSocket> {
    super::manifest::with_web_manifest(web_root, |manifest| {
        manifest
            .websockets
            .iter()
            .find(|websocket| websocket.route == request_path)
            .cloned()
    })
    .ok()
    .flatten()
}

/// WebSocket handshake classification before transport ownership begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WebSocketUpgradeState {
    /// No WebSocket handshake was attempted.
    Missing,
    /// Some WebSocket handshake headers were present, but the request is incomplete.
    Malformed,
    /// The request has the required WebSocket opening-handshake headers.
    Upgrade,
}

/// Classifies the request headers for a WebSocket upgrade.
///
/// Inputs:
/// - HTTP headers from an incoming request.
///
/// Output:
/// - Missing, malformed, or valid upgrade classification.
///
/// Transformation:
/// - Applies HTTP header-level validation before either the transitional Hyper
///   adapter or VM-stream adapter moves into WebSocket transport handling.
pub(super) fn websocket_upgrade_state(headers: &http::HeaderMap) -> WebSocketUpgradeState {
    let upgrade = headers
        .get(http::header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let connection = headers
        .get(http::header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("upgrade"))
        });
    let key = headers
        .get("sec-websocket-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.trim().is_empty());
    let version = headers
        .get("sec-websocket-version")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "13");

    if upgrade && connection && key && version {
        WebSocketUpgradeState::Upgrade
    } else if upgrade || connection || key || version {
        WebSocketUpgradeState::Malformed
    } else {
        WebSocketUpgradeState::Missing
    }
}

/// Returns whether the request headers ask for a complete WebSocket upgrade.
///
/// Inputs:
/// - HTTP headers from an incoming request.
///
/// Output:
/// - `true` when the required WebSocket upgrade headers are present.
///
/// Transformation:
/// - Keeps transitional Hyper call sites on a boolean helper while sharing the
///   stricter handshake classification with the VM-stream adapter.
#[cfg(test)]
pub(super) fn is_websocket_upgrade(headers: &http::HeaderMap) -> bool {
    websocket_upgrade_state(headers) == WebSocketUpgradeState::Upgrade
}

/// Builds the HTTP 101 switching-protocols response for a WebSocket request.
///
/// Inputs:
/// - Original HTTP request containing `sec-websocket-key`.
///
/// Output:
/// - HTTP 101 response with WebSocket upgrade headers.
///
/// Transformation:
/// - Delegates protocol response planning to the VM-owned WebSocket helper,
///   then adapts the metadata into the transitional Hyper response type.
#[cfg(test)]
pub(super) fn websocket_upgrade_response<B>(request: &Request<B>) -> Response<ServeBody> {
    let body = Full::new(Bytes::new()).boxed();
    let Some(upgrade) = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            crate::runtime::vm::websocket::build_websocket_upgrade_response(value).ok()
        })
    else {
        return Response::builder()
            .status(http::StatusCode::BAD_REQUEST)
            .body(body)
            .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()));
    };

    let status =
        http::StatusCode::from_u16(upgrade.status).unwrap_or(http::StatusCode::SWITCHING_PROTOCOLS);
    let mut builder = Response::builder().status(status);
    for (name, value) in upgrade.headers {
        builder = builder.header(name, value);
    }
    builder
        .body(body)
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

#[cfg(test)]
#[path = "websocket_test.rs"]
mod websocket_test;
