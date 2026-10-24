use super::*;

fn request() -> RequestParts {
    RequestParts {
        method: "POST".into(),
        path: "/items".into(),
        body: "body".into(),
        body_file_path: "/tmp/body".into(),
        query_string: "key=value".into(),
        params: vec![("id".into(), "42".into())],
        headers: vec![("x-test".into(), "header".into())],
        cookies: vec!["session=cookie".into()],
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
            "headers",
            "cookies",
            "body_file_path"
        ]
    );
    for mask in 0..256_u16 {
        let NativeValue::Record { name, fields } =
            request_descriptor(request(), Projection::Fields(mask << 1))
        else {
            panic!()
        };
        assert_eq!(name, "Request");
        for index in 0..8 {
            let expected = if mask & (1 << index) != 0 {
                &complete[index]
            } else {
                &empty[index]
            };
            assert_eq!(&fields[index], expected, "mask {mask} field {index}");
        }
    }
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
fn legacy_tuple_decodes_query_without_discarding_the_original_text() {
    let mut input = request();
    let raw = "key=first&key=last&empty&space=a+b&bad=%FF&eq=a=b";
    input.query_string = raw.into();
    let NativeValue::Tuple(fields) = source_request_tuple(input) else {
        panic!("request tuple");
    };
    assert_eq!(fields[5], raw.into());
    assert_eq!(
        fields[6],
        NativeValue::Map(vec![
            ("key".into(), "first".into()),
            ("key".into(), "last".into()),
            ("empty".into(), "".into()),
            ("space".into(), "a b".into()),
            ("bad".into(), "\u{fffd}".into()),
            ("eq".into(), "a=b".into()),
        ])
    );
}

#[test]
fn records_preserve_duplicate_cookies_while_tuple_maps_keep_first_values() {
    let groups: Vec<String> = vec![
        "sid=first; empty=; sid=shadowed; SID=upper; empty=later".into(),
        "".into(),
        "sid=later-header".into(),
    ];
    let expected: NativeValue = groups.clone().into();
    let mut input = request();
    input.cookies = groups.clone();
    let NativeValue::Record { fields, .. } = request_descriptor(input, Projection::Complete) else {
        panic!("request record")
    };
    assert_eq!(fields[6].1, expected);
    let mut input = request();
    input.cookies = groups;
    let NativeValue::Tuple(fields) = source_request_tuple(input) else {
        panic!("request tuple")
    };
    assert_eq!(
        fields[8],
        string_map(vec![
            ("sid".into(), "first".into()),
            ("empty".into(), "".into()),
            ("SID".into(), "upper".into()),
        ])
    );
    assert_eq!(first_cookie_map(vec![]), NativeValue::Map(vec![]));
}

#[test]
fn legacy_tuple_never_promotes_later_headers_when_first_is_empty_or_absent() {
    for groups in [
        vec![],
        vec!["".into()],
        vec!["".into(), "sid=later".into()],
        vec!["invalid".into(), "sid=later".into()],
    ] {
        let mut input = request();
        input.cookies = groups;
        let NativeValue::Tuple(fields) = source_request_tuple(input) else {
            panic!("request tuple")
        };
        assert_eq!(fields[8], NativeValue::Map(vec![]));
    }
}

#[test]
fn all_record_metadata_preserves_order_duplicates_and_empty_values() {
    let pairs = vec![
        ("key".into(), "first".into()),
        ("key".into(), "".into()),
        ("KEY".into(), "different".into()),
        ("key".into(), "last".into()),
    ];
    let mut input = request();
    input.params = pairs.clone();
    input.query_string = "key=first&key=&KEY=different&key=last".into();
    input.headers = pairs.clone();
    input.cookies = vec!["key=first; key=; KEY=different; key=last".into()];
    let expected: NativeValue = pairs.into();
    let NativeValue::Record { fields, .. } = request_descriptor(input, Projection::Complete) else {
        panic!("request record")
    };
    assert_eq!(
        fields[6].1,
        vec!["key=first; key=; KEY=different; key=last"].into()
    );
    assert_eq!(fields[4].1, "key=first&key=&KEY=different&key=last".into());
    for index in [2, 5] {
        assert_eq!(fields[index].1, expected);
    }
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
    let mut complete = Projection::Complete;
    complete.include(1);
    complete.include(usize::MAX);
    assert_eq!(complete, Projection::Complete);
}
