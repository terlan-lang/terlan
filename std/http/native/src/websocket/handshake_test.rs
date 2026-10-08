use super::*;

fn valid_headers() -> HeaderMap {
    http::Request::builder()
        .header("upgrade", "WebSocket")
        .header("connection", "keep-alive, Upgrade")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .body(())
        .unwrap()
        .into_parts()
        .0
        .headers
}

fn reject_response(
    method: &Method,
    version: Version,
    headers: &HeaderMap,
    status: u16,
) -> Response<Bytes> {
    let OpeningHandshake::Reject(response) = opening_handshake(method, version, headers) else {
        panic!("invalid opening request upgraded: {method} {version:?} {headers:?}");
    };
    assert_eq!(response.status(), status, "{headers:?}");
    assert!(!response.headers().contains_key("sec-websocket-accept"));
    response
}

#[test]
fn maintained_handshake_accepts_browser_headers_and_split_connection_lines() {
    for split in [false, true] {
        let mut headers = valid_headers();
        if split {
            headers.insert(
                header::CONNECTION,
                http::HeaderValue::from_static("keep-alive"),
            );
            headers.append(
                header::CONNECTION,
                http::HeaderValue::from_static("Upgrade"),
            );
        }
        let OpeningHandshake::Upgrade(response) =
            opening_handshake(&Method::GET, Version::HTTP_11, &headers)
        else {
            panic!("valid upgrade rejected");
        };
        assert_eq!(response.status(), 101);
        assert_eq!(
            response.headers()["sec-websocket-accept"],
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert_eq!(response.headers()["upgrade"], "websocket");
        assert_eq!(response.headers()["connection"], "Upgrade");
        assert!(response.body().is_empty());
    }
}

#[test]
fn missing_and_non_get_requests_preserve_route_errors_and_head_length() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONNECTION,
        http::HeaderValue::from_static("keep-alive"),
    );
    let missing = reject_response(&Method::GET, Version::HTTP_11, &headers, 426);
    assert_eq!(missing.headers()["upgrade"], "websocket");
    assert_eq!(missing.body().as_ref(), b"websocket upgrade required");
    for method in [Method::POST, Method::HEAD, Method::CONNECT] {
        let response = reject_response(&method, Version::HTTP_11, &valid_headers(), 405);
        assert_eq!(response.headers()["allow"], "GET");
        assert_eq!(response.headers()["upgrade"], "websocket");
        assert_eq!(response.headers()["content-length"], "30");
        assert_eq!(response.body().is_empty(), method == Method::HEAD);
    }
}

#[test]
fn malformed_handshakes_reject_before_channel_admission() {
    for name in [
        "upgrade",
        "connection",
        "sec-websocket-key",
        "sec-websocket-version",
    ] {
        let mut headers = valid_headers();
        headers.remove(name);
        reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
        let mut headers = valid_headers();
        headers.insert(name, http::HeaderValue::from_static("wrong"));
        reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
        headers.insert(name, http::HeaderValue::from_bytes(b"\xff").unwrap());
        reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
    }
    for version in [
        Version::HTTP_09,
        Version::HTTP_10,
        Version::HTTP_2,
        Version::HTTP_3,
    ] {
        reject_response(&Method::GET, version, &valid_headers(), 400);
    }
}

#[test]
fn duplicate_singletons_and_invalid_nonces_never_upgrade() {
    for name in ["upgrade", "sec-websocket-key", "sec-websocket-version"] {
        for duplicate_valid in [false, true] {
            let mut headers = valid_headers();
            let value = if duplicate_valid {
                headers[name].clone()
            } else {
                http::HeaderValue::from_static("wrong")
            };
            headers.append(name, value);
            reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
        }
    }
    for key in [
        "",
        " ",
        "invalid",
        "AAAAAAAAAAAAAAAAAAAAAA=A",
        "AAAAAAAAAAAAAAAAAAAAAA== ",
    ] {
        let mut headers = valid_headers();
        headers.insert(
            "sec-websocket-key",
            http::HeaderValue::from_str(key).unwrap(),
        );
        reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
    }
    for length in 0..=32 {
        let mut headers = valid_headers();
        let key = base64::engine::general_purpose::STANDARD.encode(vec![0; length]);
        headers.insert(
            "sec-websocket-key",
            http::HeaderValue::from_str(&key).unwrap(),
        );
        if length == 16 {
            assert!(matches!(
                opening_handshake(&Method::GET, Version::HTTP_11, &headers),
                OpeningHandshake::Upgrade(_)
            ));
        } else {
            reject_response(&Method::GET, Version::HTTP_11, &headers, 400);
        }
    }
}
