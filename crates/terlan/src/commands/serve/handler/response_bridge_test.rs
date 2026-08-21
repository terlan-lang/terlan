//! Tests for compiler-owned managed HTTP response decoding.

use super::*;

/// Builds one uniform managed response tuple emitted by direct AOT code.
fn response(kind: i64, payload: &str, status: i64) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::Int(kind),
        ReplValue::String(payload.to_string()),
        ReplValue::Int(status),
        ReplValue::String(String::new()),
        ReplValue::List(Vec::new()),
    ])
}

/// Verifies managed response metadata preserves repeated header order.
#[test]
fn native_repeated_headers_are_validated_and_preserved() {
    let mut value = response(0, "cookies", 200);
    if let ReplValue::Tuple(fields) = &mut value {
        fields[5] = ReplValue::List(vec![
            ReplValue::Tuple(vec![
                ReplValue::String("Set-Cookie".to_string()),
                ReplValue::String("a=1".to_string()),
            ]),
            ReplValue::Tuple(vec![
                ReplValue::String("Set-Cookie".to_string()),
                ReplValue::String("b=2".to_string()),
            ]),
        ]);
    } else {
        unreachable!("response helper returns tuple");
    }
    let decoded =
        HandlerResponse::from_vm_response_inner(&value, None).expect("decode repeated headers");
    assert_eq!(
        decoded.headers,
        vec![
            ("Set-Cookie".to_string(), "a=1".to_string()),
            ("Set-Cookie".to_string(), "b=2".to_string()),
        ]
    );

    if let ReplValue::Tuple(fields) = &mut value {
        fields[5] = ReplValue::String("not-a-list".to_string());
    }
    assert_eq!(
        HandlerResponse::from_vm_response_inner(&value, None).unwrap_err(),
        "error[serve_handler]: native Response headers must be List[Header]"
    );
}

/// Verifies application security policy headers cross the response bridge.
#[test]
fn native_security_headers_are_not_claimed_by_transport_framing() {
    for (name, value) in [
        ("X-Content-Type-Options", "nosniff"),
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "strict-origin-when-cross-origin"),
        ("Strict-Transport-Security", "max-age=31536000"),
    ] {
        assert_eq!(
            validate_response_header(name, value),
            Ok((name.to_string(), value.to_string()))
        );
    }
}

/// Verifies cache policy is selected by the application rather than rejected
/// as HTTP framing metadata.
#[test]
fn native_cache_control_header_crosses_the_response_bridge() {
    assert_eq!(
        validate_response_header("Cache-Control", "public, max-age=60"),
        Ok((
            "Cache-Control".to_string(),
            "public, max-age=60".to_string()
        ))
    );
}

/// Verifies native text, HTML, and JSON responses preserve media and body semantics.
#[test]
fn native_body_responses_decode_from_uniform_managed_layout() {
    for (kind, content_type) in [
        (0, "text/plain; charset=utf-8"),
        (1, "text/html; charset=utf-8"),
        (2, "application/json; charset=utf-8"),
    ] {
        let decoded =
            HandlerResponse::from_vm_response_inner(&response(kind, "managed body", 207), None)
                .expect("decode native response");
        assert_eq!(decoded.status, 207);
        assert_eq!(decoded.content_type, content_type);
        assert_eq!(decoded.body.as_bytes(), b"managed body");
        assert!(decoded.headers.is_empty());
    }
}

/// Verifies the immediate AOT bridge can consume the managed response body.
#[test]
fn owned_native_body_response_preserves_uniform_layout() {
    let decoded = HandlerResponse::from_owned_vm_response_with_package_root(
        response(0, "owned body", 206),
        Path::new("."),
    )
    .expect("decode owned native response");
    assert_eq!(decoded.status, 206);
    assert_eq!(decoded.content_type, "text/plain; charset=utf-8");
    assert_eq!(decoded.body.as_bytes(), b"owned body");
    assert!(decoded.headers.is_empty());
}

/// Verifies redirect metadata and unknown discriminants remain explicit.
#[test]
fn native_redirect_and_unknown_kind_are_checked() {
    let decoded = HandlerResponse::from_vm_response_inner(&response(3, "/next", 308), None)
        .expect("decode redirect");
    assert_eq!(decoded.status, 308);
    assert_eq!(
        decoded.headers,
        vec![("Location".to_string(), "/next".to_string())]
    );
    assert_eq!(
        HandlerResponse::from_vm_response_inner(&response(99, "bad", 200), None).unwrap_err(),
        "error[serve_handler]: unsupported native Response kind `99`"
    );
}

/// Verifies the direct AOT envelope preserves body, headers, and status.
#[test]
fn typed_aot_response_skips_generic_vm_materialization() {
    let decoded = HandlerResponse::from_aot_http_response(VmAotHttpResponse {
        kind: 2,
        status: 201,
        payload: Bytes::from_static(br#"{"created":true}"#),
        headers: vec![("X-Request-Id".to_string(), "abc".to_string())],
    })
    .expect("decode typed AOT response");
    assert_eq!(decoded.status, 201);
    assert_eq!(decoded.content_type, "application/json; charset=utf-8");
    assert_eq!(decoded.body.as_bytes(), br#"{"created":true}"#);
    assert_eq!(
        decoded.headers,
        vec![("X-Request-Id".to_string(), "abc".to_string())]
    );
}

/// Verifies typed redirects and malformed values retain boundary validation.
#[test]
fn typed_aot_response_validates_redirect_and_protocol_fields() {
    let decoded = HandlerResponse::from_aot_http_response(VmAotHttpResponse {
        kind: 3,
        status: 308,
        payload: Bytes::from_static(b"/next"),
        headers: Vec::new(),
    })
    .expect("decode typed redirect");
    assert!(decoded.body.is_empty());
    assert_eq!(
        decoded.headers,
        vec![("Location".to_string(), "/next".to_string())]
    );

    let invalid = HandlerResponse::from_aot_http_response(VmAotHttpResponse {
        kind: 0,
        status: 99,
        payload: Bytes::from_static(b"bad"),
        headers: Vec::new(),
    })
    .unwrap_err();
    assert!(invalid.contains("outside HTTP range"));

    let invalid = HandlerResponse::from_aot_http_response(VmAotHttpResponse {
        kind: 0,
        status: 200,
        payload: Bytes::from_static(b"bad"),
        headers: vec![("Bad Header".to_string(), "value".to_string())],
    })
    .unwrap_err();
    assert!(invalid.contains("not a valid HTTP token"));
}
