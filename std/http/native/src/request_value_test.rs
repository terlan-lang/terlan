use super::*;

fn request() -> RequestParts {
    RequestParts {
        method: "POST".into(),
        path: "/items".into(),
        body: "body".into(),
        body_file_path: "/tmp/body".into(),
        query_string: "key=value".into(),
        params: vec![("id".into(), "42".into())],
        query: vec![("key".into(), "value".into())],
        headers: vec![("x-test".into(), "header".into())],
        cookies: vec![("session".into(), "cookie".into())],
    }
}

#[test]
fn every_projection_selects_only_its_field_and_preserves_record_shape() {
    let NativeValue::Record {
        fields: complete, ..
    } = request_descriptor(request(), Projection::Complete)
    else {
        panic!()
    };
    let NativeValue::Record { fields: empty, .. } =
        request_descriptor(request(), Projection::empty())
    else {
        panic!()
    };
    assert_eq!(
        complete
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "method",
            "path",
            "params",
            "body",
            "query_string",
            "query",
            "headers",
            "cookies",
            "cookie_jar",
            "body_file_path"
        ]
    );
    for mask in 0..1024_u16 {
        let NativeValue::Record { name, fields } =
            request_descriptor(request(), Projection::Fields(mask << 1))
        else {
            panic!()
        };
        assert_eq!(name, "Request");
        for index in 0..10 {
            let expected = if mask & (1 << index) != 0 {
                &complete[index]
            } else {
                &empty[index]
            };
            assert_eq!(&fields[index], expected, "mask {mask} field {index}");
        }
    }
    assert!(
        matches!(&complete[8].1, NativeValue::Record { name, fields }
        if name == "Jar" && fields[0].1 == complete[7].1
            && fields[1] == ("pending".into(), NativeValue::List(vec![])))
    );
}

#[test]
fn source_tuple_preserves_the_public_pattern_head_order() {
    let NativeValue::Tuple(fields) = source_request_tuple(request()) else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            NativeValue::Atom("request".into()),
            "POST".into(),
            "/items".into(),
            string_map(vec![("id".into(), "42".into())]),
            "body".into(),
            "key=value".into(),
            string_map(vec![("key".into(), "value".into())]),
            string_map(vec![("x-test".into(), "header".into())]),
            string_map(vec![("session".into(), "cookie".into())]),
            "/tmp/body".into(),
        ]
    );
}

#[test]
fn projection_overflow_fails_closed_including_large_machine_word_indexes() {
    for field in [16, 32, usize::MAX] {
        let mut projection = Projection::empty();
        assert!(!projection.requires(field));
        projection.include(field);
        assert_eq!(projection, Projection::Complete);
        assert!(projection.requires(field));
    }
    let mut projection = Projection::empty();
    projection.include(15);
    assert!(projection.requires(15));
    assert!(!projection.requires(14));
}
