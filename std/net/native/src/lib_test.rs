use super::*;

fn parsed(text: &str) -> NativeValue {
    parse_parts(&[NativeValue::String(text.into())]).expect("valid binding arguments")
}

fn optional_text(value: Option<&str>) -> NativeValue {
    value.map(str::to_owned).into()
}

#[test]
fn normalized_components_are_owned_values() {
    assert_eq!(
        parsed("https://EXAMPLE.com/a/../docs?q=terlan#intro"),
        NativeValue::from(Ok::<_, String>(NativeValue::Record {
            name: "Uri".into(),
            fields: vec![
                (
                    "as_str".into(),
                    "https://example.com/docs?q=terlan#intro".into()
                ),
                ("scheme".into(), "https".into()),
                ("host_str".into(), optional_text(Some("example.com"))),
                ("path".into(), "/docs".into()),
                ("query".into(), optional_text(Some("q=terlan"))),
                ("fragment".into(), optional_text(Some("intro"))),
            ],
        }))
    );
}

#[test]
fn absence_is_distinct_from_empty_components() {
    for (text, host, query, fragment) in [
        ("https://example.com/docs", Some("example.com"), None, None),
        (
            "https://example.com/docs?#",
            Some("example.com"),
            Some(""),
            Some(""),
        ),
        ("mailto:user@example.com", None, None, None),
        ("https://[::1]:8080/", Some("[::1]"), None, None),
    ] {
        let NativeValue::Record { fields, .. } = parsed(text) else {
            panic!("result")
        };
        let NativeValue::Record { fields: parts, .. } = &fields[0].1 else {
            panic!("components")
        };
        assert_eq!(parts[2], ("host_str".into(), optional_text(host)));
        assert_eq!(parts[4], ("query".into(), optional_text(query)));
        assert_eq!(parts[5], ("fragment".into(), optional_text(fragment)));
    }
}

#[test]
fn malformed_text_is_a_value_error_not_a_boundary_failure() {
    for text in [
        "not a uri",
        "://missing-scheme",
        "https://[::1",
        "https://exa mple.com",
        "http://",
        "",
    ] {
        let NativeValue::Record { name, fields } = parsed(text) else {
            panic!("result")
        };
        assert_eq!(name, "Err", "{text}");
        assert!(matches!(&fields[0].1, NativeValue::String(message) if !message.is_empty()));
    }
}

#[test]
fn native_entrypoint_validates_arity_and_types() {
    for arguments in [
        vec![],
        vec![NativeValue::Int(42)],
        vec![
            NativeValue::String("https://example.com".into()),
            NativeValue::Unit,
        ],
    ] {
        assert_eq!(
            (PARSE.invoke)(&arguments).unwrap_err().code(),
            "native_package.arguments"
        );
        assert_eq!(
            (QUERY_PAIRS.invoke)(&arguments).unwrap_err().code(),
            if arguments.len() == 1 {
                "dispatch.type"
            } else {
                "native_package.arguments"
            }
        );
    }
}

#[test]
fn query_binding_preserves_wire_order_duplicates_and_maintained_decoding() {
    for (query, expected) in [
        ("", vec![]),
        ("a=1&a=2", vec![("a", "1"), ("a", "2")]),
        (
            "room=a+b&player=one%2Btwo",
            vec![("room", "a b"), ("player", "one+two")],
        ),
        (
            "empty&=value&bad=%ZZ",
            vec![("empty", ""), ("", "value"), ("bad", "%ZZ")],
        ),
    ] {
        let expected = NativeValue::from(expected);
        assert_eq!(NativeValue::from(query_pairs(query)), expected);
        assert_eq!((QUERY_PAIRS.invoke)(&[query.into()]).unwrap(), expected);
    }
}

#[test]
fn maintained_parser_preserves_normalization_and_encoding() {
    for text in [
        "https://example.com/%2F?q=%23#%3F",
        "https://example.com/?one=1&one=2",
        "https://user:password@example.com:443/",
        "https://example.com/\u{754c}",
    ] {
        let NativeValue::Record { fields, .. } = parsed(text) else {
            panic!("result")
        };
        let NativeValue::Record { fields: parts, .. } = &fields[0].1 else {
            panic!("components")
        };
        let NativeValue::String(normalized) = &parts[0].1 else {
            panic!("text")
        };
        assert_eq!(parsed(normalized), parsed(text));
    }
}
