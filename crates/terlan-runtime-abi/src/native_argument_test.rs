use super::*;

#[test]
fn owned_text_bool_and_float_conversions_preserve_types_without_coercion() {
    let owned = {
        let value = NativeValue::String("owned\0text".into());
        String::from_native(&value).unwrap()
    };
    assert_eq!(owned, "owned\0text");
    for value in [false, true] {
        assert_eq!(bool::from_native(&NativeValue::Bool(value)).unwrap(), value);
    }
    for value in [
        f64::MIN,
        -0.0,
        0.0,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        let native = NativeValue::from(value);
        assert_eq!(
            f64::from_native(&native).unwrap().to_bits(),
            value.to_bits()
        );
    }
    for input in [
        NativeValue::Unit,
        NativeValue::Int(1),
        NativeValue::Bytes(b"secret".to_vec()),
        NativeValue::Atom("secret".into()),
    ] {
        for error in [
            String::from_native(&input).unwrap_err(),
            bool::from_native(&input).unwrap_err(),
            f64::from_native(&input).unwrap_err(),
        ] {
            assert_eq!(error.code(), "dispatch.type");
            assert!(!error.to_string().contains("secret"));
        }
    }
    assert!(bool::from_native(&NativeValue::Float(1.0)).is_err());
    assert!(f64::from_native(&NativeValue::Bool(true)).is_err());
    assert!(String::from_native(&NativeValue::Bool(true)).is_err());
    assert!(String::from_native(&NativeValue::Float(1.0)).is_err());
    assert!(bool::from_native(&NativeValue::String("true".into())).is_err());
    assert!(f64::from_native(&NativeValue::String("1.0".into())).is_err());
}

#[test]
fn bytes_and_lists_borrow_payloads_and_reject_coercion() {
    let bytes = NativeValue::Bytes(vec![0, 255, 128]);
    let NativeValue::Bytes(original) = &bytes else {
        unreachable!()
    };
    assert_eq!(
        <&[u8]>::from_native(&bytes).unwrap().as_ptr(),
        original.as_ptr()
    );
    assert!(<&[u8]>::from_native(&NativeValue::from("abc")).is_err());
    let values = NativeValue::List(vec!["first".into(), "second".into()]);
    let NativeValue::List(original) = &values else {
        unreachable!()
    };
    let borrowed = Vec::<&str>::from_native(&values).unwrap();
    assert_eq!(borrowed, ["first", "second"]);
    assert_eq!(
        borrowed[0].as_ptr(),
        <&str>::from_native(&original[0]).unwrap().as_ptr()
    );
    assert!(Vec::<i64>::from_native(&NativeValue::List(vec![]))
        .unwrap()
        .is_empty());
    let nested = NativeValue::List(vec![NativeValue::List(vec![NativeValue::Int(1)])]);
    assert_eq!(
        Vec::<Vec<i64>>::from_native(&nested).unwrap(),
        vec![vec![1]]
    );
    for invalid in [
        NativeValue::Tuple(vec![]),
        bytes,
        NativeValue::List(vec!["first".into(), NativeValue::Int(2)]),
    ] {
        assert!(Vec::<&str>::from_native(&invalid)
            .unwrap_err()
            .to_string()
            .contains("dispatch.type"));
    }
}

#[test]
fn scalar_arguments_borrow_without_coercion() {
    let text = NativeValue::from("sensitive");
    let NativeValue::String(original) = &text else {
        unreachable!()
    };
    assert_eq!(
        <&str>::from_native(&text).unwrap().as_ptr(),
        original.as_ptr()
    );
    for number in [i64::MIN, 0, i64::MAX] {
        assert_eq!(i64::from_native(&NativeValue::Int(number)).unwrap(), number);
    }
    for value in [
        NativeValue::Unit,
        NativeValue::Bool(true),
        NativeValue::Float(1.0),
        NativeValue::Bytes(b"sensitive".to_vec()),
        NativeValue::Atom("sensitive".into()),
        NativeValue::Tuple(vec![]),
        NativeValue::List(vec![]),
        NativeValue::Map(vec![]),
    ] {
        for error in [
            <&str>::from_native(&value).unwrap_err(),
            i64::from_native(&value).unwrap_err(),
        ] {
            assert!(error.to_string().contains("dispatch.type"));
            assert!(!error.to_string().contains("sensitive"));
        }
    }
    assert!(<&str>::from_native(&NativeValue::Int(1)).is_err());
    assert!(i64::from_native(&text).is_err());
}

#[test]
fn option_arguments_roundtrip_and_reject_malformed_records() {
    for input in [None, Some(""), Some("value")] {
        let value = NativeValue::from(input);
        assert_eq!(Option::<&str>::from_native(&value).unwrap(), input);
    }
    let nested = NativeValue::from(Some(Some("nested")));
    assert_eq!(
        Option::<Option<&str>>::from_native(&nested).unwrap(),
        Some(Some("nested"))
    );
    for (name, fields) in [
        ("None", vec![("value", NativeValue::Unit)]),
        ("Some", vec![]),
        ("Some", vec![("wrong", NativeValue::from("value"))]),
        ("Some", vec![("value", NativeValue::Int(1))]),
        (
            "Some",
            vec![
                ("value", NativeValue::from("a")),
                ("value", NativeValue::from("b")),
            ],
        ),
        ("Other", vec![]),
    ] {
        let value = NativeValue::Record {
            name: name.into(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        };
        assert!(Option::<&str>::from_native(&value).is_err(), "{value:?}");
    }
    assert!(Option::<&str>::from_native(&NativeValue::Unit).is_err());
}
