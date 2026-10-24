//! Legacy in-memory transport test adapters. Production uses package admission.
#![cfg(test)]

use http_body_util::{BodyExt, Full};
use hyper::{Request, Response};

use super::ServeBody;

pub(super) fn websocket_upgrade_response<B>(request: &Request<B>) -> Response<ServeBody> {
    use terlan_http_native::websocket::handshake::{opening_handshake, OpeningHandshake};
    let response = match opening_handshake(request.method(), request.version(), request.headers()) {
        OpeningHandshake::Upgrade(response) | OpeningHandshake::Reject(response) => response,
    };
    response.map(|body| {
        Full::new(body)
            .map_err(|never| match never {})
            .boxed_unsync()
    })
}

#[path = "websocket_test.rs"]
mod tests;
