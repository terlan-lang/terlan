//! Test-runner-only access to production HTTP request and response values.
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeResult};
use crate::runtime::native::http::{self, RequestFieldProjection};
use crate::runtime::vm::native_value::from_native;

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
#[path = "test_fixtures_test.rs"]
mod tests;
