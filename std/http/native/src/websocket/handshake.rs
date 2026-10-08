//! HTTP/1 opening admission before source callbacks acquire channel resources.

use base64::Engine as _;
use bytes::Bytes;
use http::{header, HeaderMap, Method, Response, Version};

pub enum OpeningHandshake {
    Upgrade(Response<Bytes>),
    Reject(Response<Bytes>),
}

/// Protocol parsing and accept-key construction belong to tungstenite. The
/// package adds bounded nonce validation and rejects ambiguous singleton fields.
pub fn opening_handshake(
    method: &Method,
    version: Version,
    headers: &HeaderMap,
) -> OpeningHandshake {
    if method != Method::GET {
        return reject(
            405,
            "websocket upgrades require GET",
            &[("allow", "GET"), ("upgrade", "websocket")],
            method == Method::HEAD,
        );
    }
    let attempted = [
        "upgrade",
        "sec-websocket-key",
        "sec-websocket-version",
        "sec-websocket-protocol",
        "sec-websocket-extensions",
    ]
    .iter()
    .any(|name| headers.contains_key(*name));
    if !attempted {
        return reject(
            426,
            "websocket upgrade required",
            &[("upgrade", "websocket")],
            false,
        );
    }
    if version == Version::HTTP_11
        && ["upgrade", "sec-websocket-key", "sec-websocket-version"]
            .iter()
            .all(|name| headers.get_all(*name).iter().count() == 1)
        && headers.get("sec-websocket-key").is_some_and(|key| {
            key.as_bytes().len() == 24
                && base64::engine::general_purpose::STANDARD
                    .decode(key.as_bytes())
                    .is_ok_and(|nonce| nonce.len() == 16)
        })
    {
        let mut request = http::Request::new(());
        *request.version_mut() = version;
        *request.headers_mut() = headers.clone();
        // Connection is a list-valued header; preserve all lines for the
        // maintained token parser instead of silently inspecting only the first.
        let connection = headers
            .get_all(header::CONNECTION)
            .iter()
            .collect::<Vec<_>>();
        if connection.len() > 1 {
            let joined = connection
                .iter()
                .map(|value| value.as_bytes())
                .collect::<Vec<_>>()
                .join(&b',');
            let value = http::HeaderValue::from_bytes(&joined)
                .expect("joining validated header values with a comma is valid");
            request.headers_mut().insert(header::CONNECTION, value);
        }
        if let Ok(response) =
            tungstenite::handshake::server::create_response_with_body(&request, Bytes::new)
        {
            return OpeningHandshake::Upgrade(response);
        }
    }
    reject(400, "malformed websocket upgrade request", &[], false)
}

fn reject(
    status: u16,
    message: &'static str,
    headers: &[(&str, &str)],
    head_only: bool,
) -> OpeningHandshake {
    let headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    OpeningHandshake::Reject(
        crate::build_server_response(
            status,
            "text/plain; charset=utf-8",
            &headers,
            Bytes::from_static(message.as_bytes()),
            head_only,
            false,
        )
        .expect("package handshake rejection metadata is constant and valid"),
    )
}

#[cfg(test)]
#[path = "handshake_test.rs"]
mod tests;
