use super::*;
use terlan_runtime_abi::NativeValue as V;

fn fields() -> Vec<(String, V)> {
    vec![
        ("kind".into(), V::Int(0)),
        ("payload".into(), V::String("body".into())),
        ("status".into(), V::Int(218)),
        (
            "content_type".into(),
            V::String("application/custom".into()),
        ),
        (
            "headers".into(),
            V::List(vec![
                ("X-Test", "first").into(),
                ("X-Test", "second").into(),
            ]),
        ),
        (
            "chunks".into(),
            V::List(vec!["abcdef".into(), "".into(), "ghi".into()]),
        ),
        ("chunk_size".into(), V::Int(4)),
        ("max_pending_writes".into(), V::Int(2)),
    ]
}

fn value(fields: Vec<(String, V)>) -> V {
    V::Record {
        name: "Response".into(),
        fields,
    }
}

#[test]
fn cached_response_projection_preserves_metadata_and_still_requires_admission() {
    for (content_type, expected_kind) in [
        ("text/plain", 0),
        ("text/html; charset=utf-8", 1),
        ("application/json", 0),
    ] {
        let descriptor = cached_response(
            207,
            content_type.into(),
            "cached".into(),
            vec![("X-Cache".into(), "hit".into())],
        );
        let V::Record {
            ref name,
            ref fields,
        } = descriptor
        else {
            panic!("record");
        };
        assert_eq!(name, "Response");
        assert_eq!(
            fields.iter().find(|(key, _)| key == "kind").unwrap().1,
            V::Int(expected_kind)
        );
        let parsed = response(descriptor).unwrap();
        assert_eq!(parsed.status, 207);
        assert_eq!(parsed.content_type, content_type);
        assert_eq!(parsed.headers, [("X-Cache".into(), "hit".into())]);
        assert!(matches!(parsed.body, SourceResponseBody::Text(body) if body == "cached"));
    }
    for descriptor in [
        cached_response(600, "text/plain".into(), "body".into(), vec![]),
        cached_response(
            200,
            "text/plain\r\nInjected: yes".into(),
            "body".into(),
            vec![],
        ),
        cached_response(
            200,
            "text/plain".into(),
            "body".into(),
            vec![("Content-Length".into(), "1".into())],
        ),
    ] {
        assert!(response(descriptor).is_err());
    }
}

#[test]
fn source_response_preserves_allocations_metadata_and_repeated_header_order() {
    for kind in 0..=2 {
        let mut fields = fields();
        fields[0].1 = V::Int(kind);
        let V::String(body) = &fields[1].1 else {
            panic!("body");
        };
        let body_pointer = body.as_ptr();
        let V::List(headers) = &fields[4].1 else {
            panic!("headers");
        };
        let V::Tuple(header) = &headers[0] else {
            panic!("header");
        };
        let V::String(header_value) = &header[1] else {
            panic!("value");
        };
        let header_pointer = header_value.as_ptr();
        fields.reverse();
        let parsed = response(value(fields)).unwrap();
        assert_eq!(parsed.status, 218);
        assert_eq!(parsed.content_type, "application/custom");
        assert_eq!(
            parsed.headers,
            [
                ("X-Test".into(), "first".into()),
                ("X-Test".into(), "second".into())
            ]
        );
        assert_eq!(parsed.headers[0].1.as_ptr(), header_pointer);
        let SourceResponseBody::Text(body) = parsed.body else {
            panic!("text");
        };
        assert_eq!(body, "body");
        assert_eq!(body.as_ptr(), body_pointer);
    }
}

#[test]
fn source_response_rejects_wrong_missing_unknown_duplicate_and_mistyped_fields() {
    for malformed in [
        V::Unit,
        V::Tuple(vec![]),
        V::Record {
            name: "Other".into(),
            fields: fields(),
        },
    ] {
        assert_eq!(response(malformed).unwrap_err().code(), "http.descriptor");
    }
    for index in 0..8 {
        let mut missing = fields();
        missing.remove(index);
        let mut unknown = fields();
        unknown[index].0 = "unknown".into();
        let mut extra = fields();
        extra.push(extra[index].clone());
        let mut duplicate = fields();
        duplicate[index].0 = duplicate[(index + 1) % 8].0.clone();
        let mut mistyped = fields();
        mistyped[index].1 = V::Bool(false);
        for fields in [missing, unknown, extra, duplicate, mistyped] {
            assert_eq!(
                response(value(fields)).unwrap_err().code(),
                "http.descriptor"
            );
        }
    }
}

#[test]
fn source_response_rejects_unsafe_metadata_and_invalid_body_kinds() {
    for (index, invalid) in [
        (0, V::Int(3)),
        (0, V::Int(6)),
        (0, V::Int(-1)),
        (2, V::Int(i64::MIN)),
        (2, V::Int(99)),
        (2, V::Int(600)),
        (2, V::Int(i64::MAX)),
        (3, V::String("text/plain\r\nInjected: yes".into())),
        (4, V::List(vec![V::Unit])),
        (4, V::List(vec![V::Tuple(vec![])])),
        (4, V::List(vec![("name",).into()])),
        (4, V::List(vec![("name", "value", "extra").into()])),
        (4, V::List(vec![("name", 42_i64).into()])),
        (4, V::List(vec![("X-Bad", "bad\r\nvalue").into()])),
        (4, V::List(vec![("bad name", "value").into()])),
        (4, V::List(vec![("Content-Length", "99").into()])),
    ] {
        let mut fields = fields();
        fields[index].1 = invalid;
        assert!(response(value(fields)).is_err());
    }
    for status in [100, 599] {
        let mut fields = fields();
        fields[2].1 = V::Int(status);
        assert_eq!(response(value(fields)).unwrap().status, status as u16);
    }
}

#[test]
fn source_response_streams_remain_bounded_and_files_require_host_authority() {
    let mut file = fields();
    file[0].1 = V::Int(4);
    file[1].1 = V::String("../not-authorized-by-decoder".into());
    assert!(
        matches!(response(value(file)).unwrap().body, SourceResponseBody::File(path) if path == "../not-authorized-by-decoder")
    );
    let mut stream = fields();
    stream[0].1 = V::Int(5);
    let SourceResponseBody::Stream(mut chunks) = response(value(stream.clone())).unwrap().body
    else {
        panic!("stream");
    };
    for expected in ["abcd", "ef", "ghi"] {
        assert_eq!(chunks.next_chunk().unwrap(), expected);
    }
    assert!(chunks.next_chunk().is_none());
    for (index, invalid) in [
        (5, V::List(vec![V::Int(1)])),
        (6, V::Int(0)),
        (6, V::Int(-1)),
        (7, V::Int(0)),
        (7, V::Int(-1)),
    ] {
        let mut fields = stream.clone();
        fields[index].1 = invalid;
        assert!(response(value(fields)).is_err());
    }
}
