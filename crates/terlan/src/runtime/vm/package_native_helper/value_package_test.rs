use super::*;

#[test]
fn owned_values_roundtrip_without_domain_specific_types() {
    let values = vec![
        ReplValue::Unit,
        ReplValue::Bool(true),
        ReplValue::Int(-42),
        ReplValue::Float("1.25".into()),
        ReplValue::String("text".into()),
        ReplValue::Bytes(vec![0, 255].into()),
        ReplValue::Atom("named".into()),
        ReplValue::Tuple(vec![ReplValue::Int(1)]),
        ReplValue::List(vec![ReplValue::String("item".into())]),
        ReplValue::Record {
            name: "Value".into(),
            fields: vec![("data".into(), ReplValue::Bool(false))],
        },
    ];
    for value in values {
        assert_eq!(from_native(to_native(&value).unwrap()), value);
    }
    assert_eq!(
        from_native(to_native(&ReplValue::StringBytes(b"text".to_vec().into())).unwrap()),
        ReplValue::String("text".into())
    );
}

#[test]
fn invalid_scalar_representations_are_rejected() {
    assert!(to_native(&ReplValue::Float("invalid".into())).is_err());
    assert!(to_native(&ReplValue::StringBytes(vec![255].into())).is_err());
}

#[test]
fn registered_arity_is_checked_before_conversion_or_invocation() {
    let operation = NativeBinding {
        operation: "application.identity",
        arity: 1,
        invoke: |arguments| Ok(arguments[0].clone()),
    };
    for arguments in [
        vec![],
        vec![ReplValue::Unit, ReplValue::Float("invalid".into())],
    ] {
        let request = PureNativeCapabilityRequest {
            capability: "package-native".into(),
            operation: operation.operation.into(),
            arguments: vec![],
            package_arguments: Some(arguments),
            result_type: crate::runtime::native_image::TvmBoundaryType::Unit,
        };
        let error = call(&operation, &request).unwrap_err().to_string();
        assert!(error.contains("expects 1 arguments"), "{error}");
    }
}

#[test]
fn package_registry_has_unique_operation_identities() {
    let mut names = std::collections::BTreeSet::new();
    for binding in packages::VALUE_BINDINGS {
        assert!(
            names.insert(binding.operation),
            "duplicate package operation"
        );
    }
    assert!(binding("unknown.operation").is_none());
}

#[test]
fn package_calls_require_decoded_arguments_and_preserve_typed_results() {
    let parser = binding("std.net.uri.parse_parts").unwrap();
    let mut request = PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: parser.operation.into(),
        arguments: Vec::new(),
        package_arguments: None,
        result_type: crate::runtime::native_image::TvmBoundaryType::Unit,
    };
    assert!(call(parser, &request)
        .unwrap_err()
        .to_string()
        .contains("no decoded arguments"));
    request.package_arguments = Some(vec![ReplValue::Int(42)]);
    assert!(call(parser, &request)
        .unwrap_err()
        .to_string()
        .contains("requires one String"));
    request.package_arguments = Some(vec![ReplValue::String("relative/path".into())]);
    assert!(
        matches!(call(parser, &request).unwrap(), ReplValue::Record { name, fields }
        if name == "Err" && matches!(&fields[..], [(field, ReplValue::String(_))] if field == "reason"))
    );
    request.package_arguments = Some(vec![ReplValue::String("https://example.com/".into())]);
    assert!(
        matches!(call(parser, &request).unwrap(), ReplValue::Record { name, fields }
        if name == "Ok" && matches!(&fields[..], [(field, ReplValue::Record { name, fields })]
            if field == "value" && name == "Uri" && fields.len() == 6 && fields[0].0 == "as_str"))
    );
}
