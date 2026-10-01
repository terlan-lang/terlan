use super::*;

fn response_with_headers(headers: Vec<(String, String)>) -> ReplValue {
    from_native(terlan_http_native::source_descriptor::cached_response(
        200,
        "text/plain".into(),
        "ok".into(),
        headers,
    ))
}

#[test]
fn response_header_fixture_reads_source_headers_only_in_test_runner() {
    let mut request = PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: "std.test.fixture.http_response_header".into(),
        arguments: vec![],
        package_arguments: Some(vec![
            response_with_headers(vec![
                ("Set-Cookie".into(), "terlan_session=abc; HttpOnly".into()),
                ("set-cookie".into(), "second=must-not-replace-first".into()),
            ]),
            ReplValue::String("set-cookie".into()),
        ]),
        result_type: crate::runtime::native_image::TvmBoundaryType::String,
    };
    let mut application = super::super::VmPackageNativeHelpers::default();
    assert!(application
        .call(7, &request, &[])
        .unwrap_err()
        .contains("fixture_disabled"));
    let mut runner = super::super::VmPackageNativeHelpers::from_helper_environment(&[]).unwrap();
    assert_eq!(
        runner.call(7, &request, &[]).unwrap(),
        ReplValue::String("terlan_session=abc; HttpOnly".into())
    );
    request.package_arguments.as_mut().unwrap()[1] = ReplValue::String("missing".into());
    assert_eq!(
        runner.call(7, &request, &[]).unwrap(),
        ReplValue::String(String::new())
    );
    request.package_arguments.as_mut().unwrap()[0] = ReplValue::Tuple(vec![]);
    assert!(runner
        .call(7, &request, &[])
        .unwrap_err()
        .contains("expected source Response record"));
}

#[test]
fn response_header_fixture_rejects_malformed_source_headers() {
    let valid = ReplValue::Tuple(vec![
        ReplValue::String("Set-Cookie".into()),
        ReplValue::String("session=abc".into()),
    ]);
    for (name, fields) in [
        ("Other", vec![("headers".into(), ReplValue::List(vec![]))]),
        ("Response", vec![]),
        ("Response", vec![("headers".into(), ReplValue::Int(0))]),
        (
            "Response",
            vec![
                ("headers".into(), ReplValue::List(vec![])),
                ("headers".into(), ReplValue::List(vec![])),
            ],
        ),
        (
            "Response",
            vec![(
                "headers".into(),
                ReplValue::List(vec![valid.clone(), ReplValue::Unit]),
            )],
        ),
        (
            "Response",
            vec![(
                "headers".into(),
                ReplValue::List(vec![valid, ReplValue::Tuple(vec![ReplValue::Int(0)])]),
            )],
        ),
    ] {
        let request = PureNativeCapabilityRequest {
            capability: "package-native".into(),
            operation: "std.test.fixture.http_response_header".into(),
            arguments: vec![],
            package_arguments: Some(vec![
                ReplValue::Record {
                    name: name.into(),
                    fields,
                },
                ReplValue::String("Set-Cookie".into()),
            ]),
            result_type: crate::runtime::native_image::TvmBoundaryType::String,
        };
        assert!(call(true, &request)
            .unwrap_err()
            .contains("test.fixture_arguments"));
    }
}

#[test]
fn response_header_fixture_uses_complete_package_admission_before_lookup() {
    let response = response_with_headers(vec![("Set-Cookie".into(), "session=abc".into())]);
    for (field, replacement) in [
        ("status", ReplValue::Int(600)),
        (
            "content_type",
            ReplValue::String("text/plain\r\nX: y".into()),
        ),
        ("kind", ReplValue::Int(3)),
        ("headers", ReplValue::List(vec![ReplValue::Unit])),
        ("payload", ReplValue::Unit),
    ] {
        let mut invalid = response.clone();
        let ReplValue::Record { fields, .. } = &mut invalid else {
            panic!("response record");
        };
        fields.iter_mut().find(|(key, _)| key == field).unwrap().1 = replacement;
        assert_fixture_rejects(invalid);
    }
    for headers in [
        vec![("Set-Cookie".into(), "session=abc\r\nInjected: true".into())],
        vec![
            ("Set-Cookie".into(), "session=abc".into()),
            ("Content-Length".into(), "2".into()),
        ],
    ] {
        // A matching first header must not bypass validation of later metadata.
        assert_fixture_rejects(response_with_headers(headers));
    }
}

fn assert_fixture_rejects(response: ReplValue) {
    let request = PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: "std.test.fixture.http_response_header".into(),
        arguments: vec![],
        package_arguments: Some(vec![response, ReplValue::String("Set-Cookie".into())]),
        result_type: crate::runtime::native_image::TvmBoundaryType::String,
    };
    assert!(call(true, &request)
        .unwrap_err()
        .contains("test.fixture_arguments"));
}

#[test]
fn http_fixture_is_unavailable_to_application_helpers() {
    let request = PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: "std.test.fixture.http_request".into(),
        arguments: vec![],
        package_arguments: Some(vec![ReplValue::String("session=abc123".into())]),
        result_type: crate::runtime::native_image::TvmBoundaryType::Unit,
    };
    let mut application = super::super::VmPackageNativeHelpers::default();
    assert!(application
        .call(7, &request, &[])
        .unwrap_err()
        .contains("fixture_disabled"));
    let mut runner = super::super::VmPackageNativeHelpers::from_helper_environment(&[]).unwrap();
    let value = runner.call(7, &request, &[]).unwrap();
    let ReplValue::Record { name, fields } = value else {
        panic!("source-owned request record");
    };
    assert_eq!(name, "Request");
    assert_eq!(fields.len(), 10);
    assert_eq!(fields[7].0, "cookies");
    assert_eq!(
        fields[7].1,
        ReplValue::Map(vec![(
            ReplValue::String("session".into()),
            ReplValue::String("abc123".into())
        )])
    );
}
