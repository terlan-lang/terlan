//! Test-runner-only access to production HTTP request and response values.
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeResult};
use crate::runtime::native::http::{self, RequestFieldProjection};
use crate::runtime::vm::http_request_value::vm_request_descriptor_owned;

pub(super) fn call(
    enabled: bool,
    request: &PureNativeCapabilityRequest,
) -> VmRuntimeResult<ReplValue> {
    if !enabled {
        return Err(
            "error[test.fixture_disabled]: runtime fixtures require the test runner".into(),
        );
    }
    match (
        request.operation.as_str(),
        request.package_arguments.as_deref(),
    ) {
        ("std.test.fixture.http_request", Some([ReplValue::String(cookie_header)])) => {
            let request = http::Request::from_parts_with_raw_query_metadata(
                "GET",
                "/",
                "",
                http::RequestMetadata {
                    params: vec![],
                    query_string: "".into(),
                    query: vec![],
                    headers: vec![("cookie".into(), cookie_header.clone())],
                    cookies: http::parse_request_cookie_header(cookie_header),
                },
            );
            Ok(vm_request_descriptor_owned(
                request.into_parts(),
                RequestFieldProjection::Complete,
            ))
        }
        (
            "std.test.fixture.http_response_header",
            Some([ReplValue::Tuple(fields), ReplValue::String(name)]),
        ) => {
            let [ReplValue::Int(0), ReplValue::Int(_), _, ReplValue::Int(_), ReplValue::String(_), ReplValue::List(headers), ..] =
                fields.as_slice()
            else {
                return Err(
                    "error[test.fixture_arguments]: expected a managed HTTP response".into(),
                );
            };
            for header in headers {
                let ReplValue::Tuple(pair) = header else {
                    return Err("error[test.fixture_arguments]: malformed response header".into());
                };
                let [ReplValue::String(key), ReplValue::String(value)] = pair.as_slice() else {
                    return Err("error[test.fixture_arguments]: malformed response header".into());
                };
                if key.eq_ignore_ascii_case(name) {
                    return Ok(ReplValue::String(value.clone()));
                }
            }
            Ok(ReplValue::String(String::new()))
        }
        _ => Err("error[test.fixture_arguments]: unknown fixture or invalid argument shape".into()),
    }
}

#[cfg(test)]
mod tests {
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
        let mut runner =
            super::super::VmPackageNativeHelpers::from_helper_environment(&[]).unwrap();
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
        let mut runner =
            super::super::VmPackageNativeHelpers::from_helper_environment(&[]).unwrap();
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
}
