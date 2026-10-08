use super::*;
use std::path::{Path, PathBuf};
use terlan_runtime_abi::{native_record, NativeValue};

fn source(kind: i64, payload: String, content_type: &str) -> NativeValue {
    let status = 218_i64;
    let headers = vec![
        ("X-Test".to_owned(), "first".to_owned()),
        ("X-Test".to_owned(), "second".to_owned()),
    ];
    let default_headers = Vec::<(String, String)>::new();
    let chunks = vec!["abc".to_owned(), "de".to_owned()];
    let chunk_size = 2_i64;
    let max_pending_writes = 1_i64;
    native_record!(Response, {kind, payload, status, content_type, default_headers, headers, chunks, chunk_size, max_pending_writes})
}

#[test]
fn source_text_uses_one_allocation_through_admission_and_transport() {
    let payload = "source-owned text".repeat(128);
    let pointer = payload.as_ptr();
    let admitted = admit(source(0, payload, "text/custom"), None, &[]).unwrap();
    assert_eq!(admitted.body.as_bytes().unwrap().as_ptr(), pointer);
    let response = admitted.into_http(false).unwrap();
    assert_eq!(response.body().as_ptr(), pointer);
    assert_eq!(response.status().as_u16(), 218);
    assert_eq!(response.headers()["content-type"], "text/custom");
    assert_eq!(
        response.headers()["content-length"],
        response.body().len().to_string()
    );
    assert_eq!(
        response
            .headers()
            .get_all("x-test")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    for name in [
        "connection",
        "cache-control",
        "x-content-type-options",
        "transfer-encoding",
    ] {
        assert!(!response.headers().contains_key(name));
    }
}

#[test]
fn finite_head_preserves_byte_length_and_does_not_emit_the_body() {
    for body in [
        ResponseBody::Text("\u{e9}".into()),
        ResponseBody::Bytes(vec![0xff, 0]),
    ] {
        let admitted = AdmittedResponse {
            status: 200,
            content_type: "application/octet-stream".into(),
            headers: vec![],
            body,
        };
        assert_eq!(admitted.body.as_bytes().unwrap().len(), 2);
        let response = admitted.into_http(true).unwrap();
        assert!(response.body().is_empty());
        assert_eq!(response.headers()["content-length"], "2");
        assert!(response.extensions().get::<HttpResponseChunks>().is_none());
    }
}

#[test]
fn file_admission_preserves_binary_bytes_and_transfers_the_allocation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("body.txt"), [0xff, 0, 10]).unwrap();
    for content_type in ["", "application/custom"] {
        let admitted = admit(
            source(4, "body.txt".into(), content_type),
            Some(root.path()),
            &[],
        )
        .unwrap();
        assert_eq!(
            admitted.content_type,
            if content_type.is_empty() {
                "text/plain; charset=utf-8"
            } else {
                content_type
            }
        );
        let pointer = admitted.body.as_bytes().unwrap().as_ptr();
        let response = admitted.into_http(false).unwrap();
        assert_eq!(response.body().as_ptr(), pointer);
        assert_eq!(response.body().as_ref(), &[0xff, 0, 10]);
    }
}

#[test]
fn file_access_requires_explicit_context_and_capabilities() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("private.txt");
    std::fs::write(&path, "private").unwrap();
    let value = source(4, path.to_str().unwrap().into(), "text/plain");
    assert_eq!(
        admit(value.clone(), None, &[]).unwrap_err().code(),
        "http.file"
    );
    assert!(admit(value.clone(), Some(root.path()), &[]).is_err());
    let admitted = admit(value, Some(root.path()), &[outside.path().into()]).unwrap();
    assert_eq!(admitted.body.as_bytes(), Some(b"private".as_slice()));
    assert!(admit(
        source(4, "../outside".into(), "text/plain"),
        Some(root.path()),
        &[]
    )
    .is_err());
}

#[test]
fn stream_transport_is_pull_driven_and_head_never_installs_work() {
    for head in [false, true] {
        let admitted = admit(source(5, String::new(), "text/event-stream"), None, &[]).unwrap();
        assert_eq!(admitted.body.as_bytes(), None);
        let mut response = admitted.into_http(head).unwrap();
        assert!(response.body().is_empty());
        assert!(!response.headers().contains_key("content-length"));
        assert!(!response.headers().contains_key("transfer-encoding"));
        let stream = response.extensions_mut().remove::<HttpResponseChunks>();
        if head {
            assert!(stream.is_none());
        } else {
            let mut stream = stream.unwrap();
            for chunk in ["ab", "c", "de"] {
                assert_eq!(stream.next_chunk().unwrap().as_ref(), chunk.as_bytes());
            }
            assert!(stream.next_chunk().is_none());
        }
    }
}

#[test]
fn malformed_source_metadata_is_rejected_before_file_access() {
    let NativeValue::Record {
        name,
        fields: original,
    } = source(4, "not-present".into(), "text/plain")
    else {
        panic!("record")
    };
    for (field, invalid) in [
        ("status", NativeValue::Int(99)),
        (
            "content_type",
            NativeValue::String("text/plain\r\nx: y".into()),
        ),
        (
            "headers",
            NativeValue::List(vec![NativeValue::Tuple(vec![
                NativeValue::String("Content-Length".into()),
                NativeValue::String("8".into()),
            ])]),
        ),
    ] {
        let mut fields = original.clone();
        fields.iter_mut().find(|(key, _)| key == field).unwrap().1 = invalid;
        let error = admit(
            NativeValue::Record {
                name: name.clone(),
                fields,
            },
            None,
            &[],
        )
        .unwrap_err();
        assert_eq!(error.code(), "http.descriptor");
    }
}

#[test]
fn transport_revalidates_metadata_even_for_host_constructed_responses() {
    for (status, content_type, headers) in [
        (0, "text/plain", vec![]),
        (200, "text/plain\r\nx: y", vec![]),
        (200, "text/plain", vec![("bad name".into(), "x".into())]),
        (200, "text/plain", vec![("X-Test".into(), "x\r\ny".into())]),
    ] {
        let response = AdmittedResponse {
            status,
            content_type: content_type.into(),
            headers,
            body: ResponseBody::Text(String::new()),
        };
        assert!(response.into_http(false).is_err());
    }
}

fn admit(
    value: NativeValue,
    root: Option<&Path>,
    trusted_roots: &[PathBuf],
) -> Result<AdmittedResponse, HttpError> {
    AdmittedResponse::from_source(value, |path, content_type| {
        crate::file_response::read_response_file(root, trusted_roots, path, content_type)
    })
}

#[test]
fn finite_stream_and_malformed_descriptors_never_request_file_capabilities() {
    for (value, accepted) in [
        (source(0, "text".into(), "text/plain"), true),
        (source(5, String::new(), "text/plain"), true),
        (NativeValue::Bool(false), false),
    ] {
        let result = AdmittedResponse::from_source(value, |_, _| {
            panic!("file capabilities requested without a valid file intent")
        });
        assert_eq!(result.is_ok(), accepted);
    }
}

#[test]
fn authorized_reader_runs_once_and_its_failure_is_not_replaced() {
    let mut calls = 0;
    let error = AdmittedResponse::from_source(
        source(4, "requested".into(), "text/custom"),
        |path, content_type| {
            calls += 1;
            assert_eq!(path, "requested");
            assert_eq!(content_type, "text/custom");
            Err(HttpError::new("test.file_denied", "denied", 403))
        },
    )
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error.code(), "test.file_denied");
    assert_eq!(error.status(), 403);
}
