use super::NativeValue;

#[test]
fn borrowed_text_conversion_owns_exact_utf8_and_embedded_nuls() {
    let converted = {
        let text = String::from("\u{754c}\0value");
        NativeValue::from(text.as_str())
    };
    assert_eq!(converted, NativeValue::String("\u{754c}\0value".into()));
    assert_eq!(NativeValue::from(""), NativeValue::String(String::new()));
}

#[test]
fn tuple_conversion_preserves_order_and_arity() {
    let cases = [
        NativeValue::from(("a",)),
        NativeValue::from(("a", "b")),
        NativeValue::from(("a", "b", "c")),
        NativeValue::from(("a", "b", "c", "d")),
        NativeValue::from(("a", "b", "c", "d", "e")),
        NativeValue::from(("a", "b", "c", "d", "e", "f")),
    ];
    for (index, converted) in cases.into_iter().enumerate() {
        assert_eq!(
            converted,
            NativeValue::Tuple(
                ["a", "b", "c", "d", "e", "f"][..=index]
                    .iter()
                    .map(|text| NativeValue::String((*text).into()))
                    .collect()
            )
        );
    }
}

#[test]
fn nested_borrowed_tuple_conversion_keeps_types_and_owns_payloads() {
    let converted = {
        let text = String::from("payload");
        NativeValue::from((
            (text.as_str(), NativeValue::Int(42)),
            Some(""),
            None::<&str>,
            Ok::<_, &str>(text.as_str()),
            Err::<&str, _>(text.as_str()),
            NativeValue::Atom("tag".into()),
        ))
    };
    assert_eq!(
        converted,
        NativeValue::Tuple(vec![
            NativeValue::Tuple(vec![
                NativeValue::String("payload".into()),
                NativeValue::Int(42)
            ]),
            NativeValue::Record {
                name: "Some".into(),
                fields: vec![("value".into(), NativeValue::String("".into()))]
            },
            NativeValue::Record {
                name: "None".into(),
                fields: vec![]
            },
            NativeValue::Record {
                name: "Ok".into(),
                fields: vec![("value".into(), NativeValue::String("payload".into()))]
            },
            NativeValue::Record {
                name: "Err".into(),
                fields: vec![("reason".into(), NativeValue::String("payload".into()))]
            },
            NativeValue::Atom("tag".into()),
        ])
    );
}

#[test]
fn option_conversion_keeps_absence_distinct_from_empty_text() {
    assert_eq!(
        NativeValue::from(None::<String>),
        NativeValue::Record {
            name: "None".into(),
            fields: vec![],
        }
    );
    assert_eq!(
        NativeValue::from(Some(String::new())),
        NativeValue::Record {
            name: "Some".into(),
            fields: vec![("value".into(), NativeValue::String(String::new()))],
        }
    );
}

#[test]
fn result_conversion_preserves_nested_option_and_error_payloads() {
    assert_eq!(
        NativeValue::from(Ok::<_, String>(Some("text".to_string()))),
        NativeValue::Record {
            name: "Ok".into(),
            fields: vec![(
                "value".into(),
                NativeValue::Record {
                    name: "Some".into(),
                    fields: vec![("value".into(), NativeValue::String("text".into()))],
                }
            )],
        }
    );
    assert_eq!(
        NativeValue::from(Err::<String, _>("reason".to_string())),
        NativeValue::Record {
            name: "Err".into(),
            fields: vec![("reason".into(), NativeValue::String("reason".into()))],
        }
    );
}
#[test]
fn native_binding_validates_exact_arity_without_invoking_the_operation() {
    let binding = super::NativeBinding {
        operation: "package.example",
        arity: 1,
        invoke: |_| Ok(super::NativeValue::Unit),
    };
    assert!(binding.validate_arity(1).is_ok());
    for received in [0, 2, usize::MAX] {
        let error = binding.validate_arity(received).unwrap_err();
        assert_eq!(error.code(), "native_package.arguments");
        assert!(error.to_string().contains("package.example"));
    }
    assert_eq!(
        (binding.invoke)(&[super::NativeValue::Unit]).unwrap(),
        super::NativeValue::Unit
    );
}
