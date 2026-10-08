//! Source response admission and retirement of compiler-owned response layouts.

use super::*;
use crate::runtime::vm::native_value::from_native;
use terlan_http_native::source_descriptor::cached_response;

fn response(content_type: &str, payload: &str, status: u16) -> ReplValue {
    from_native(cached_response(
        status,
        content_type.into(),
        payload.into(),
        vec![],
        vec![],
    ))
}

fn field_mut<'a>(value: &'a mut ReplValue, name: &str) -> &'a mut ReplValue {
    let ReplValue::Record { fields, .. } = value else {
        panic!("source response record");
    };
    &mut fields.iter_mut().find(|(key, _)| key == name).unwrap().1
}

/// Verifies source response metadata preserves repeated header order.
#[test]
fn native_repeated_headers_are_validated_and_preserved() {
    let mut value = response("text/plain", "cookies", 200);
    *field_mut(&mut value, "headers") = ReplValue::List(vec![
        ReplValue::Tuple(
            vec!["Set-Cookie", "a=1"]
                .into_iter()
                .map(|s| ReplValue::String(s.into()))
                .collect(),
        ),
        ReplValue::Tuple(
            vec!["Set-Cookie", "b=2"]
                .into_iter()
                .map(|s| ReplValue::String(s.into()))
                .collect(),
        ),
    ]);
    let decoded = source_response::decode(value.clone(), None).unwrap();
    assert_eq!(
        decoded.headers,
        [
            ("Set-Cookie".into(), "a=1".into()),
            ("Set-Cookie".into(), "b=2".into())
        ]
    );
    *field_mut(&mut value, "headers") = ReplValue::String("not-a-list".into());
    assert!(source_response::decode(value, None)
        .unwrap_err()
        .contains("malformed source Response record"));
}

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
            Ok((name.into(), value.into()))
        );
    }
}

#[test]
fn native_cache_control_header_crosses_the_response_bridge() {
    assert_eq!(
        validate_response_header("Cache-Control", "public, max-age=60"),
        Ok(("Cache-Control".into(), "public, max-age=60".into()))
    );
}

#[test]
fn owned_header_validation_retains_original_string_allocations() {
    let name = "X-Owned".to_string();
    let value = "original value".to_string();
    let name_ptr = name.as_ptr();
    let value_ptr = value.as_ptr();
    let response = from_native(cached_response(
        200,
        "text/plain".into(),
        "body".into(),
        vec![(name, value)],
        vec![],
    ));
    let decoded =
        crate::commands::serve::handler::decode_owned_response(response, Path::new(".")).unwrap();
    assert_eq!(decoded.headers[0].0.as_ptr(), name_ptr);
    assert_eq!(decoded.headers[0].1.as_ptr(), value_ptr);
}

#[test]
fn source_body_responses_preserve_provider_metadata() {
    for content_type in [
        "text/plain; charset=utf-8",
        "text/html; charset=utf-8",
        "application/json; charset=utf-8",
    ] {
        let decoded =
            source_response::decode(response(content_type, "managed body", 207), None).unwrap();
        assert_eq!(decoded.status, 207);
        assert_eq!(decoded.content_type, content_type);
        assert_eq!(
            decoded.body.as_bytes().expect("finite response"),
            b"managed body"
        );
        assert!(decoded.headers.is_empty());
    }
}

#[test]
fn owned_source_body_response_preserves_allocation() {
    let body = String::from("owned body");
    let pointer = body.as_ptr();
    let value = from_native(cached_response(
        206,
        "text/plain".into(),
        body,
        vec![],
        vec![],
    ));
    let decoded =
        crate::commands::serve::handler::decode_owned_response(value, Path::new(".")).unwrap();
    assert_eq!(decoded.status, 206);
    assert_eq!(decoded.content_type, "text/plain");
    let HandlerBody::Text(body) = decoded.body else {
        panic!("text");
    };
    assert_eq!(body, "owned body");
    assert_eq!(body.as_ptr(), pointer);
}

#[test]
fn source_redirect_metadata_and_unknown_kind_are_checked() {
    let value = from_native(cached_response(
        308,
        "text/plain".into(),
        "".into(),
        vec![("Location".into(), "/next".into())],
        vec![],
    ));
    let decoded = source_response::decode(value, None).unwrap();
    assert_eq!(decoded.status, 308);
    assert_eq!(decoded.headers, [("Location".into(), "/next".into())]);
    assert!(decoded.body.as_bytes().expect("finite response").is_empty());
    let mut unknown = response("text/plain", "bad", 200);
    *field_mut(&mut unknown, "kind") = ReplValue::Int(99);
    assert!(source_response::decode(unknown, None)
        .unwrap_err()
        .contains("unsupported source Response kind `99`"));
}

#[test]
fn retired_tuple_layouts_reject_before_body_or_metadata_interpretation() {
    for (kind, tag) in ["text", "html", "json_text", "redirect", "file", "stream"]
        .into_iter()
        .enumerate()
    {
        let payload = vec![
            ReplValue::String("../not-a-file-to-open".into()),
            ReplValue::Int(200),
            ReplValue::String("text/plain".into()),
            ReplValue::List(vec![]),
            ReplValue::List(vec![ReplValue::String("chunk".into())]),
            ReplValue::Int(1),
            ReplValue::Int(1),
        ];
        for prefix in [
            vec![ReplValue::Int(0), ReplValue::Int(kind as i64)],
            vec![
                ReplValue::Atom("response".into()),
                ReplValue::Atom(tag.into()),
            ],
        ] {
            let value = ReplValue::Tuple(prefix.into_iter().chain(payload.clone()).collect());
            let borrowed = crate::commands::serve::handler::decode_response(&value, Path::new("."))
                .unwrap_err();
            let owned =
                crate::commands::serve::handler::decode_owned_response(value, Path::new("."))
                    .unwrap_err();
            assert_eq!(
                borrowed,
                "error[serve_handler]: expected source Response record"
            );
            assert_eq!(owned, borrowed);
        }
    }
}

#[test]
fn retired_response_handles_reject_even_when_disguised_as_source_records() {
    for (id, generation) in [(0, 0), (1, 1), (i64::MAX, i64::MAX)] {
        let handle_fields = vec![
            ("$native_owner".into(), ReplValue::String("7".into())),
            ("$native_id".into(), ReplValue::Int(id)),
            ("$native_generation".into(), ReplValue::Int(generation)),
            (
                "$native_type".into(),
                ReplValue::String("std.http.Response.Response".into()),
            ),
        ];
        let handle = ReplValue::Record {
            name: "Response".into(),
            fields: handle_fields.clone(),
        };
        let mut mixed = response("text/plain", "not admitted", 200);
        let ReplValue::Record { fields, .. } = &mut mixed else {
            unreachable!()
        };
        fields.extend(handle_fields);
        for value in [handle, mixed] {
            let borrowed = crate::commands::serve::handler::decode_response(&value, Path::new("."))
                .unwrap_err();
            let owned =
                crate::commands::serve::handler::decode_owned_response(value, Path::new("."))
                    .unwrap_err();
            assert_eq!(
                borrowed,
                "error[serve_handler]: malformed source Response record"
            );
            assert_eq!(owned, borrowed);
        }
    }
}
