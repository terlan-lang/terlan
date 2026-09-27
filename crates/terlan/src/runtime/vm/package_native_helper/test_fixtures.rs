//! Test-runner-only values using the production HTTP parser and request layout.
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
        _ => Err("error[test.fixture_arguments]: unknown fixture or invalid argument shape".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
