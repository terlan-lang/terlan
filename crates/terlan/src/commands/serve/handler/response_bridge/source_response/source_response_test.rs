use super::*;

fn response() -> Vec<(String, ReplValue)> {
    vec![
        ("kind".into(), ReplValue::Int(0)),
        ("payload".into(), ReplValue::String("body".into())),
        ("status".into(), ReplValue::Int(218)),
        (
            "content_type".into(),
            ReplValue::String("application/custom".into()),
        ),
        (
            "headers".into(),
            ReplValue::List(vec![
                ReplValue::Tuple(vec![
                    ReplValue::String("X-Test".into()),
                    ReplValue::String("first".into()),
                ]),
                ReplValue::Tuple(vec![
                    ReplValue::String("X-Test".into()),
                    ReplValue::String("second".into()),
                ]),
            ]),
        ),
        (
            "chunks".into(),
            ReplValue::List(vec![ReplValue::String("chunk".into())]),
        ),
        ("chunk_size".into(), ReplValue::Int(4)),
        ("max_pending_writes".into(), ReplValue::Int(2)),
    ]
}

#[test]
fn transport_respects_source_metadata_and_field_names_not_offsets() {
    for kind in 0..=2 {
        let mut fields = response();
        fields[0].1 = ReplValue::Int(kind);
        fields.reverse();
        let value = decode("Response", fields, None).unwrap();
        assert_eq!(value.status, 218);
        assert_eq!(value.content_type, "application/custom");
        assert_eq!(value.body.as_bytes(), b"body");
        assert_eq!(
            value.headers,
            [
                ("X-Test".into(), "first".into()),
                ("X-Test".into(), "second".into())
            ]
        );
    }
}

#[test]
fn transport_rejects_missing_duplicate_unknown_and_mistyped_fields() {
    assert!(decode("Unrelated", response(), None).is_err());
    for index in 0..8 {
        let mut fields = response();
        fields.remove(index);
        assert!(decode("Response", fields, None).is_err());
        let mut fields = response();
        fields[index].0 = "unknown".into();
        assert!(decode("Response", fields, None).is_err());
        let mut fields = response();
        fields[index].1 = ReplValue::Bool(false);
        assert!(decode("Response", fields, None).is_err());
        let mut fields = response();
        fields.push(fields[index].clone());
        assert!(decode("Response", fields, None).is_err());
    }
}

#[test]
fn transport_rejects_unsafe_status_headers_and_stream_limits() {
    for (index, value) in [
        (0, ReplValue::Int(3)),
        (0, ReplValue::Int(6)),
        (2, ReplValue::Int(-1)),
        (2, ReplValue::Int(99)),
        (2, ReplValue::Int(1000)),
        (3, ReplValue::String("text/plain\r\nInjected: yes".into())),
        (
            4,
            ReplValue::List(vec![ReplValue::Tuple(vec![
                ReplValue::String("X-Test".into()),
                ReplValue::String("bad\r\nvalue".into()),
            ])]),
        ),
    ] {
        let mut fields = response();
        fields[index].1 = value;
        assert!(decode("Response", fields, None).is_err());
    }
    for (index, value) in [
        (5, ReplValue::List(vec![ReplValue::Int(1)])),
        (6, ReplValue::Int(0)),
        (7, ReplValue::Int(0)),
    ] {
        let mut fields = response();
        fields[0].1 = ReplValue::Int(5);
        fields[index].1 = value;
        assert!(decode("Response", fields, None).is_err());
    }
    let mut fields = response();
    fields[0].1 = ReplValue::Int(5);
    assert!(matches!(
        decode("Response", fields, None).unwrap().body,
        HandlerBody::Stream(_)
    ));
}

#[test]
fn source_files_use_the_existing_file_safety_boundary() {
    let mut fields = response();
    fields[0].1 = ReplValue::Int(4);
    fields[1].1 = ReplValue::String("../outside".into());
    assert!(decode("Response", fields.clone(), None)
        .unwrap_err()
        .contains("file-serving context"));
    assert!(decode("Response", fields, Some(Path::new("/tmp"))).is_err());
}

#[test]
fn source_file_responses_preserve_bytes_metadata_and_owned_decode() {
    let root = tempfile::tempdir().unwrap();
    let bytes = [0, 255, 42, 10];
    std::fs::write(root.path().join("body.txt"), bytes).unwrap();
    for content_type in ["", "application/custom"] {
        let mut fields = response();
        fields[0].1 = ReplValue::Int(4);
        fields[1].1 = ReplValue::String("body.txt".into());
        fields[3].1 = ReplValue::String(content_type.into());
        let record = ReplValue::Record {
            name: "Response".into(),
            fields,
        };
        let borrowed = HandlerResponse::from_vm_response_with_package_root(&record, root.path())
            .expect("decode package-relative file");
        let owned = HandlerResponse::from_owned_vm_response_with_package_root(record, root.path())
            .expect("consume package-relative file");
        assert_eq!(owned, borrowed);
        assert_eq!(owned.status, 218);
        assert_eq!(owned.body.as_bytes(), bytes);
        assert_eq!(owned.headers.len(), 2);
        assert_eq!(
            owned.content_type,
            if content_type.is_empty() {
                native_http::content_type_for_path(&root.path().join("body.txt"))
            } else {
                content_type.to_string()
            }
        );
    }
}
