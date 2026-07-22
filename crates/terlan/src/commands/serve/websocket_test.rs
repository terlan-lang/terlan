use http::{header, HeaderMap, HeaderValue, Request, StatusCode};

use super::*;

/// Verifies WebSocket opening-handshake headers are classified strictly.
///
/// Inputs:
/// - Browser-shaped upgrade headers with mixed casing and comma-separated
///   `Connection` tokens.
///
/// Output:
/// - Test passes when the classifier reports a complete upgrade and the legacy
///   boolean helper agrees.
///
/// Transformation:
/// - Exercises only the HTTP header boundary before any socket/frame transport
///   is selected.
#[test]
fn websocket_upgrade_state_accepts_complete_browser_handshake() {
    let headers = websocket_headers(
        Some("WebSocket"),
        Some("keep-alive, Upgrade"),
        Some("dGhlIHNhbXBsZSBub25jZQ=="),
        Some("13"),
    );

    assert_eq!(
        websocket_upgrade_state(&headers),
        WebSocketUpgradeState::Upgrade
    );
    assert!(is_websocket_upgrade(&headers));
}

/// Verifies plain HTTP requests are not treated as malformed WebSocket attempts.
///
/// Inputs:
/// - Empty HTTP header map.
///
/// Output:
/// - Test passes when the classifier reports `Missing` and the boolean helper
///   rejects the upgrade.
///
/// Transformation:
/// - Locks the `426 Upgrade Required` route boundary for manifest WebSocket
///   routes that receive normal HTTP requests.
#[test]
fn websocket_upgrade_state_reports_missing_for_plain_request() {
    let headers = HeaderMap::new();

    assert_eq!(
        websocket_upgrade_state(&headers),
        WebSocketUpgradeState::Missing
    );
    assert!(!is_websocket_upgrade(&headers));
}

/// Verifies partial WebSocket opening handshakes are reported as malformed.
///
/// Inputs:
/// - Upgrade attempts with missing version, wrong version, blank key, or missing
///   `Connection: upgrade` token.
///
/// Output:
/// - Test passes when every partial attempt is classified as malformed and not
///   accepted as a complete upgrade.
///
/// Transformation:
/// - Prevents incomplete handshakes from reaching the current VM-stream
///   transport boundary diagnostic.
#[test]
fn websocket_upgrade_state_reports_malformed_partial_handshakes() {
    let cases = [
        websocket_headers(
            Some("websocket"),
            Some("Upgrade"),
            Some("dGhlIHNhbXBsZSBub25jZQ=="),
            None,
        ),
        websocket_headers(
            Some("websocket"),
            Some("Upgrade"),
            Some("dGhlIHNhbXBsZSBub25jZQ=="),
            Some("12"),
        ),
        websocket_headers(Some("websocket"), Some("Upgrade"), Some("   "), Some("13")),
        websocket_headers(
            Some("websocket"),
            Some("keep-alive"),
            Some("dGhlIHNhbXBsZSBub25jZQ=="),
            Some("13"),
        ),
    ];

    for headers in cases {
        assert_eq!(
            websocket_upgrade_state(&headers),
            WebSocketUpgradeState::Malformed
        );
        assert!(!is_websocket_upgrade(&headers));
    }
}

/// Verifies the serve adapter reuses the VM-owned WebSocket handshake plan.
///
/// Inputs:
/// - Complete browser-shaped opening-handshake request.
///
/// Output:
/// - Test passes when the Hyper response carries the VM-planned status and
///   tungstenite-derived `Sec-WebSocket-Accept` value.
///
/// Transformation:
/// - Locks the serve layer to metadata adaptation only; WebSocket protocol
///   response planning stays under `runtime::vm::websocket`.
#[test]
fn websocket_upgrade_response_reuses_vm_handshake_plan() {
    let request = Request::builder()
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "Upgrade")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .body(())
        .expect("websocket request");

    let response = websocket_upgrade_response(&request);

    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(
        response.headers().get(header::UPGRADE),
        Some(&HeaderValue::from_static("websocket"))
    );
    assert_eq!(
        response.headers().get(header::CONNECTION),
        Some(&HeaderValue::from_static("Upgrade"))
    );
    assert_eq!(
        response.headers().get("sec-websocket-accept"),
        Some(&HeaderValue::from_static("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="))
    );
}

/// Builds a header map for WebSocket classifier tests.
///
/// Inputs:
/// - Optional upgrade, connection, key, and version header values.
///
/// Output:
/// - HTTP header map containing only the supplied values.
///
/// Transformation:
/// - Centralizes test setup while preserving exact header spelling used by
///   HTTP/WebSocket clients.
fn websocket_headers(
    upgrade: Option<&'static str>,
    connection: Option<&'static str>,
    key: Option<&'static str>,
    version: Option<&'static str>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(value) = upgrade {
        headers.insert(header::UPGRADE, HeaderValue::from_static(value));
    }
    if let Some(value) = connection {
        headers.insert(header::CONNECTION, HeaderValue::from_static(value));
    }
    if let Some(value) = key {
        headers.insert("sec-websocket-key", HeaderValue::from_static(value));
    }
    if let Some(value) = version {
        headers.insert("sec-websocket-version", HeaderValue::from_static(value));
    }
    headers
}
