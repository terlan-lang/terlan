use http::{header, HeaderName, HeaderValue};

use crate::{build_http_response, build_server_response, validate_response_header};

#[test]
fn handler_header_names_follow_the_maintained_parser_for_every_ascii_byte() {
    for byte in 0..=127 {
        let name = format!("x{}x", char::from(byte));
        assert_eq!(
            validate_response_header(&name, "value").is_ok(),
            HeaderName::from_bytes(name.as_bytes()).is_ok(),
            "name byte {byte}"
        );
    }
    for name in ["", "\u{e9}", "x\u{1f642}", "bad header", "x:y"] {
        assert!(validate_response_header(name, "value").is_err(), "{name}");
    }
}

#[test]
fn handler_values_follow_the_maintained_parser_including_control_bytes() {
    for byte in 0..=127 {
        let value = format!("before{}after", char::from(byte));
        assert_eq!(
            validate_response_header("X-Test", &value).is_ok(),
            HeaderValue::from_str(&value).is_ok(),
            "value byte {byte}"
        );
    }
    for value in ["", "\t", " leading ", "\u{e9}", "\u{1f642}"] {
        assert!(validate_response_header("X-Test", value).is_ok());
    }
    for value in ["bad\rvalue", "bad\nvalue", "bad\r\nInjected: value"] {
        assert!(validate_response_header("X-Test", value)
            .unwrap_err()
            .message()
            .contains("line break"));
    }
}

#[test]
fn handlers_cannot_supply_framing_headers_regardless_of_case() {
    for name in [
        "Content-Type",
        "Content-Length",
        "Connection",
        "Transfer-Encoding",
        "Trailer",
    ] {
        for name in [name.to_string(), name.to_uppercase(), name.to_lowercase()] {
            let error = validate_response_header(&name, "value").unwrap_err();
            assert_eq!(error.code(), "http.response.invalid_header");
            assert_eq!(error.status(), 500);
            assert!(error.message().contains("owned by the server bridge"));
        }
    }
    for name in [
        "Cache-Control",
        "Set-Cookie",
        "X-Frame-Options",
        "Referrer-Policy",
        "Content-Encoding",
    ] {
        assert!(validate_response_header(name, "value").is_ok(), "{name}");
    }
}

#[test]
fn owned_bodies_are_transferred_and_head_retains_the_original_length() {
    let text = "owned response".to_string();
    let allocation = text.as_ptr();
    let response = build_http_response(201, "text/plain", &[], text, false, false).unwrap();
    assert_eq!(response.body().as_ptr(), allocation);
    assert_eq!(response.status(), 201);
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "14");
    assert!(!response.headers().contains_key(header::CONNECTION));

    let bytes = vec![0, 255, 42];
    let allocation = bytes.as_ptr();
    let response =
        build_http_response(200, "application/octet-stream", &[], bytes, false, true).unwrap();
    assert_eq!(response.body().as_ptr(), allocation);
    assert_eq!(response.body(), &[0, 255, 42]);
    assert_eq!(response.headers()[header::CONNECTION], "close");

    let shared = bytes::Bytes::from_static(b"unchanged");
    let allocation = shared.as_ptr();
    let response = build_http_response(200, "text/plain", &[], shared, false, false).unwrap();
    assert_eq!(response.body().as_ptr(), allocation);
    let response =
        build_http_response(200, "text/plain", &[], response.into_body(), true, false).unwrap();
    assert!(response.body().is_empty());
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "9");
}

#[test]
fn response_defaults_and_repeated_application_headers_are_preserved() {
    for content_type in [
        "text/plain; charset=utf-8",
        "text/html; charset=utf-8",
        "application/json; charset=utf-8",
        "application/octet-stream",
        "application/custom",
    ] {
        let response =
            build_server_response(200, content_type, &[], String::new(), false, false).unwrap();
        assert_eq!(response.headers()[header::CONTENT_TYPE], content_type);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    }
    let headers = vec![
        ("Set-Cookie".to_string(), "a=1".to_string()),
        ("set-cookie".to_string(), "b=2".to_string()),
        (
            "CACHE-CONTROL".to_string(),
            "public, max-age=60".to_string(),
        ),
    ];
    let response =
        build_server_response(218, "text/plain", &headers, String::new(), false, false).unwrap();
    assert_eq!(
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["a=1", "b=2"]
    );
    assert_eq!(
        response
            .headers()
            .get_all(header::CACHE_CONTROL)
            .iter()
            .count(),
        1
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=60"
    );
}

#[test]
fn invalid_response_metadata_returns_typed_errors() {
    for status in [0, 99, 1000, u16::MAX] {
        let error = build_http_response(status, "text/plain", &[], String::new(), false, false)
            .unwrap_err();
        assert_eq!(error.code(), "http.response.invalid_status");
    }
    for content_type in ["bad\nvalue", "bad\rvalue", "bad\0value", "bad\u{7f}value"] {
        let error =
            build_http_response(200, content_type, &[], String::new(), false, false).unwrap_err();
        assert_eq!(error.code(), "http.response.invalid_content_type");
    }
    for (name, value) in [
        ("bad header", "ok"),
        ("", "ok"),
        ("X-Test", "bad\nvalue"),
        ("X-Test", "bad\0value"),
    ] {
        let error = build_http_response(
            200,
            "text/plain",
            &[(name.into(), value.into())],
            String::new(),
            false,
            false,
        )
        .unwrap_err();
        assert_eq!(error.code(), "http.response.invalid_header");
    }
}

#[test]
fn source_wire_builder_never_invents_policy_headers() {
    for head in [false, true] {
        let response =
            build_http_response(200, "text/plain", &[], "body".to_string(), head, false).unwrap();
        assert!(!response.headers().contains_key("cache-control"));
        assert!(!response.headers().contains_key("x-content-type-options"));
        let headers = vec![
            ("Cache-Control".into(), "no-cache".into()),
            ("CACHE-CONTROL".into(), "private".into()),
            ("X-Content-Type-Options".into(), "package-value".into()),
        ];
        let response =
            build_http_response(200, "text/plain", &headers, "body".to_string(), head, false)
                .unwrap();
        assert_eq!(
            response
                .headers()
                .get_all("cache-control")
                .iter()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["no-cache", "private"]
        );
        assert_eq!(
            response.headers()["x-content-type-options"],
            "package-value"
        );
        assert_eq!(
            response
                .headers()
                .get_all("x-content-type-options")
                .iter()
                .count(),
            1
        );
        let server =
            build_server_response(200, "text/plain", &headers, String::new(), head, false).unwrap();
        assert_eq!(
            server
                .headers()
                .get_all("x-content-type-options")
                .iter()
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["nosniff", "package-value"]
        );
    }
    assert!(build_server_response(99, "text/plain", &[], String::new(), false, false).is_err());
}

#[test]
fn every_owned_body_representation_obeys_head_close_and_metadata_validation() {
    fn check<B: AsRef<[u8]> + Default + Clone>(body: B) {
        let headers = vec![("X-Source".into(), "kept".into())];
        for head in [false, true] {
            for close in [false, true] {
                let response =
                    build_http_response(200, "text/plain", &headers, body.clone(), head, close)
                        .unwrap();
                assert_eq!(
                    response.body().as_ref(),
                    if head { b"" } else { body.as_ref() }
                );
                assert_eq!(
                    response.headers()["content-length"],
                    body.as_ref().len().to_string()
                );
                assert_eq!(response.headers()["x-source"], "kept");
                assert_eq!(response.headers().contains_key("connection"), close);
                assert!(!response.headers().contains_key("cache-control"));
                assert!(!response.headers().contains_key("x-content-type-options"));
            }
        }
        assert!(build_http_response(99, "text/plain", &headers, body, false, false).is_err());
    }
    check("body".to_string());
    check(b"body".to_vec());
    check(bytes::Bytes::from_static(b"body"));
}
