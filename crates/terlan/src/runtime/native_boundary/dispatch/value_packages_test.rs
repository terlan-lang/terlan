use super::*;

#[test]
fn owned_values_roundtrip_without_resource_identities() {
    let value = Value::Record {
        name: "Payload".into(),
        fields: vec![(
            "values".into(),
            Value::List(vec![
                Value::Map(vec![(
                    Value::Text("key".into()),
                    Value::List(vec![Value::Bool(true)]),
                )]),
                Value::Unit,
                Value::Bool(true),
                Value::Int(-7),
                Value::Float(1.5),
                Value::Text("hello\0world".into()),
                Value::Bytes(vec![0, 255]),
                Value::Atom("ready".into()),
                Value::Tuple(vec![Value::Int(1)]),
            ]),
        )],
    };
    assert_eq!(from_native(to_native(&value).unwrap()), value);
    for value in [None, Some("".to_owned()), Some("value".to_owned())] {
        let native = to_native(&Value::OptionalText(value.clone())).unwrap();
        assert_eq!(native, NativeValue::from(value));
    }
    let resource = Value::Json(crate::terlan_native::json::null());
    assert_eq!(
        to_native(&resource).unwrap_err().code(),
        "native_package.value"
    );
    assert!(to_native(&Value::List(vec![resource])).is_err());
}

#[test]
fn dispatch_preserves_dynamic_package_error_codes() {
    let binding = NativeBinding {
        operation: "example.failure",
        arity: 0,
        invoke: |_| {
            Err(terlan_runtime_abi::BoundaryError::message(
                terlan_runtime_abi::ErrorDomain::NativeBoundary,
                "test",
                "error[package.custom_failure]: details",
            ))
        },
    };
    let error = dispatch(&binding, &[]).unwrap_err();
    assert_eq!(error.code(), "package.custom_failure");
    assert!(error.message().contains("details"));
}
