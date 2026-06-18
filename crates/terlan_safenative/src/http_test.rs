use super::*;
use crate::json as json_adapter;

/// Verifies request body JSON parsing delegates to the JSON adapter.
///
/// Inputs:
/// - A request wrapper containing valid JSON text.
///
/// Output:
/// - Test passes when the parsed value exposes the expected integer field.
///
/// Transformation:
/// - Exercises the HTTP request wrapper without depending on sockets or server
///   framework state.
#[test]
fn body_json_parses_valid_request_body() {
    let request = Request::new(r#"{"count":2}"#);
    let parsed = body_json(&request).expect("valid JSON should parse");
    let count = json_adapter::get(&parsed, "count")
        .and_then(|value| json_adapter::as_int(&value))
        .expect("count should be an integer");

    assert_eq!(count, 2);
}

/// Verifies request construction preserves HTTP method and path metadata.
///
/// Inputs:
/// - A request wrapper built from explicit method, path, and body parts.
///
/// Output:
/// - Test passes when all request fields are readable.
///
/// Transformation:
/// - Exercises the Rust-native request snapshot used by server bridge code.
#[test]
fn request_from_parts_preserves_method_path_and_body() {
    let request = Request::from_parts("POST", "/api/users", r#"{"name":"Ada"}"#);

    assert_eq!(request.method(), "POST");
    assert_eq!(request.path(), "/api/users");
    assert_eq!(request.body(), r#"{"name":"Ada"}"#);
}

/// Verifies invalid request JSON maps into an HTTP error.
///
/// Inputs:
/// - A request wrapper containing malformed JSON text.
///
/// Output:
/// - Test passes when the error has the stable body-json code and status 400.
///
/// Transformation:
/// - Converts JSON parser failure into HTTP error metadata.
#[test]
fn body_json_reports_invalid_request_body() {
    let request = Request::new("{");
    let error = body_json(&request).expect_err("invalid JSON should fail");

    assert_eq!(error.code(), "http.body_json");
    assert_eq!(error.status(), 400);
}

/// Verifies JSON response construction sets portable defaults.
///
/// Inputs:
/// - A JSON string adapter value.
///
/// Output:
/// - Test passes when the response status, content type, and body are stable.
///
/// Transformation:
/// - Serializes the JSON value into response storage without a server runtime.
#[test]
fn json_response_uses_json_defaults() {
    let response = json(&json_adapter::string("ok"));

    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "\"ok\"");
}

/// Verifies text responses can be mutated with status and headers.
///
/// Inputs:
/// - A text response wrapper.
///
/// Output:
/// - Test passes when mutable metadata updates are visible.
///
/// Transformation:
/// - Exercises mutable receiver backing behavior for response metadata.
#[test]
fn text_response_accepts_status_and_header_updates() {
    let mut response = text("created");
    status(&mut response, 201);
    header(&mut response, "x-terlan", "yes");

    assert_eq!(response.status_code(), 201);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "created");
    assert_eq!(
        response.headers(),
        &[("x-terlan".to_string(), "yes".to_string())]
    );
}

/// Verifies response construction from explicit metadata.
///
/// Inputs:
/// - Status, content type, and body values from a bridge boundary.
///
/// Output:
/// - Test passes when the response exposes the supplied values.
///
/// Transformation:
/// - Exercises the Rust-native response snapshot used by server bridge code.
#[test]
fn response_from_parts_preserves_status_content_type_and_body() {
    let response = Response::from_parts(202, "application/json; charset=utf-8", "{\"ok\":true}");

    assert_eq!(response.status_code(), 202);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "{\"ok\":true}");
    assert!(response.headers().is_empty());
}
