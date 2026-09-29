use super::*;

#[test]
fn response_header_fixture_reads_managed_headers_only_in_test_runner() {
    let mut request = PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: "std.test.fixture.http_response_header".into(),
        arguments: vec![],
        package_arguments: Some(vec![
            ReplValue::Tuple(vec![
                ReplValue::Int(0),
                ReplValue::Int(0),
                ReplValue::String("ok".into()),
                ReplValue::Int(200),
                ReplValue::String("text/plain".into()),
                ReplValue::List(vec![ReplValue::Tuple(vec![
                    ReplValue::String("Set-Cookie".into()),
                    ReplValue::String("terlan_session=abc; HttpOnly".into()),
                ])]),
                ReplValue::List(vec![]),
                ReplValue::Int(0),
                ReplValue::Int(0),
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
        .contains("expected a managed HTTP response"));
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
    let ReplValue::Tuple(fields) = value else {
        panic!("production request tuple");
    };
    assert_eq!(fields.len(), 11);
    assert_eq!(
        fields[8],
        ReplValue::Map(vec![(
            ReplValue::String("session".into()),
            ReplValue::String("abc123".into())
        )])
    );
}
