use super::*;
use std::path::Path;

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

/// Verifies the native accessor exposes only the server-owned upload path.
#[test]
fn body_file_path_reads_temporary_upload_path() {
    let request = Request::new("").with_body_file_path("/tmp/terlan-upload");

    assert_eq!(request.body_file_path(), "/tmp/terlan-upload");
}

/// Verifies request construction preserves decoded route/query/cookie metadata.
///
/// Inputs:
/// - A request wrapper built from explicit metadata pairs.
///
/// Output:
/// - Test passes when storage preserves metadata without selecting lookup policy.
///
/// Transformation:
/// - Exercises the request metadata shape used by router-backed handlers
///   without binding a socket server.
#[test]
fn request_from_parts_with_metadata_preserves_lookup_pairs() {
    let request = Request::from_parts_with_raw_query_metadata(
        "GET",
        "/users/42",
        "",
        RequestMetadata {
            params: vec![("id".to_string(), "42".to_string())],
            query_string: ("tab=profile").into(),
            query: vec![("tab".to_string(), "profile".to_string())],
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            cookies: vec![("theme".to_string(), "dark".to_string())],
        },
    );

    assert_eq!(request.method(), "GET");
    assert_eq!(request.path(), "/users/42");
    assert_eq!(request.query_string(), "tab=profile");
    let parts = request.into_parts();
    assert_eq!(parts.params, [("id".into(), "42".into())]);
    assert_eq!(parts.query, [("tab".into(), "profile".into())]);
    assert_eq!(
        parts.headers,
        [("Accept".into(), "application/json".into())]
    );
    assert_eq!(parts.cookies, [("theme".into(), "dark".into())]);
}

/// Verifies request metadata names remain text at the HTTP boundary.
///
/// Inputs:
/// - Route, query, header, and cookie names that look like Vm atom
///   construction functions.
///
/// Output:
/// - Test passes when each stored pair retains the associated text value.
///
/// Transformation:
/// - Exercises request metadata storage without converting external names into
///   atoms or runtime symbols.
#[test]
fn request_metadata_names_that_look_like_atom_builders_remain_strings() {
    let request = Request::from_parts_with_raw_query_metadata(
        "GET",
        "/debug",
        "",
        RequestMetadata {
            params: vec![("binary_to_atom".to_string(), "route".to_string())],
            query_string: ("list_to_atom=query").into(),
            query: vec![("list_to_atom".to_string(), "query".to_string())],
            headers: vec![("Binary-To-Atom".to_string(), "header".to_string())],
            cookies: vec![("list_to_atom".to_string(), "cookie".to_string())],
        },
    );

    let parts = request.into_parts();
    assert_eq!(parts.params, [("binary_to_atom".into(), "route".into())]);
    assert_eq!(parts.query, [("list_to_atom".into(), "query".into())]);
    assert_eq!(parts.headers, [("Binary-To-Atom".into(), "header".into())]);
    assert_eq!(parts.cookies, [("list_to_atom".into(), "cookie".into())]);
}

/// Verifies browser runtime asset MIME lookup stays at the HTTP adapter boundary.
///
/// Inputs:
/// - Representative browser, font, image, data, and opaque file paths.
///
/// Output:
/// - Test passes when each path maps to the expected content type.
///
/// Transformation:
/// - Pins the NativeBoundary MIME boundary backed by `mime_guess`.
#[test]
fn content_type_for_path_covers_browser_runtime_assets() {
    let cases = [
        ("index.html", "text/html; charset=utf-8"),
        ("app.css", "text/css; charset=utf-8"),
        ("app.js", "text/javascript; charset=utf-8"),
        ("app.js.map", "application/json; charset=utf-8"),
        ("data.json", "application/json; charset=utf-8"),
        ("module.wasm", "application/wasm"),
        ("font.woff", "font/woff"),
        ("font.woff2", "font/woff2"),
        ("font.ttf", "font/ttf"),
        ("font.otf", "font/otf"),
        ("image.avif", "image/avif"),
        ("asset.bin", "application/octet-stream"),
    ];

    for (path, expected) in cases {
        assert_eq!(content_type_for_path(Path::new(path)), expected, "{path}");
    }
}

/// Verifies portable requests convert into Rust `http` crate requests.
///
/// Inputs:
/// - A Terlan request wrapper with method, path, and body metadata.
///
/// Output:
/// - Test passes when the standard `http::Request` preserves those fields.
///
/// Transformation:
/// - Exercises the new Hyper-ready request boundary without starting a server.
#[test]
fn request_converts_to_rust_http_request() {
    let request = Request::from_parts("POST", "/api/users?active=true", "{\"name\":\"Ada\"}");
    let converted = request
        .to_http_request()
        .expect("valid request should convert");

    assert_eq!(converted.method(), "POST");
    assert_eq!(converted.uri(), "/api/users?active=true");
    assert_eq!(converted.body(), "{\"name\":\"Ada\"}");

    let request = Request::from_parts_with_raw_query_metadata(
        "GET",
        "/api/users",
        "",
        RequestMetadata {
            params: Vec::new(),
            query_string: ("active=true").into(),
            query: vec![("active".to_string(), "true".to_string())],
            headers: Vec::new(),
            cookies: Vec::new(),
        },
    );
    let converted = request
        .to_http_request()
        .expect("valid request with raw query should convert");

    assert_eq!(converted.uri(), "/api/users?active=true");
}

/// Verifies invalid request metadata is rejected by the Rust HTTP boundary.
///
/// Inputs:
/// - A Terlan request wrapper with an invalid method.
///
/// Output:
/// - Test passes when conversion returns the stable request error code.
///
/// Transformation:
/// - Confirms Terlan no longer relies on ad hoc request metadata acceptance
///   before crossing into Hyper-compatible server code.
#[test]
fn request_to_rust_http_rejects_invalid_method() {
    let request = Request::from_parts("BAD METHOD", "/", "");
    let error = request
        .to_http_request()
        .expect_err("invalid method should fail");

    assert_eq!(error.code(), "http.request.invalid");
    assert_eq!(error.status(), 400);
}
