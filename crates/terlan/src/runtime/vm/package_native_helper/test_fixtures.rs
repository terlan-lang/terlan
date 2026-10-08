//! Test-runner-only access to production HTTP request and response values.
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeResult};
use crate::runtime::vm::native_value::from_native;
use terlan_http_native::{self as http, RequestFieldProjection};

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
            Ok(from_native(terlan_http_native::request_descriptor(
                request.into_parts(),
                RequestFieldProjection::Complete,
            )))
        }
        ("std.test.fixture.http_response_header", Some([response, ReplValue::String(name)])) => {
            let response = terlan_http_native::source_descriptor::response(response.clone())
                .map_err(|error| format!("error[test.fixture_arguments]: {}", error.message()))?;
            Ok(ReplValue::String(
                response
                    .headers
                    .into_iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value)
                    .unwrap_or_default(),
            ))
        }
        _ => Err("error[test.fixture_arguments]: unknown fixture or invalid argument shape".into()),
    }
}

#[cfg(test)]
#[path = "test_fixtures_test.rs"]
mod tests;
