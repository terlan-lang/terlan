//! Legacy in-memory transport test adapters. Production uses package admission.
#![cfg(test)]

use std::path::Path;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::{Request, Response};

use super::handler::WebPackageWebSocket;
use super::ServeBody;

pub(super) type WebSocketHub = Arc<()>;

pub(super) fn websocket_hub() -> WebSocketHub {
    Arc::new(())
}

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

pub(super) fn websocket_upgrade_response<B>(request: &Request<B>) -> Response<ServeBody> {
    use terlan_http_native::websocket::handshake::{opening_handshake, OpeningHandshake};
    let response = match opening_handshake(request.method(), request.version(), request.headers()) {
        OpeningHandshake::Upgrade(response) | OpeningHandshake::Reject(response) => response,
    };
    response.map(|body| Full::new(body).boxed())
}

#[path = "websocket_test.rs"]
mod tests;
