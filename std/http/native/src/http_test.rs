use super::*;
use std::path::Path;
use terlan_data_native as json_adapter;

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
/// - Test passes when helper accessors return present and absent optional
///   values predictably.
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
    assert_eq!(request.param("id"), Some("42".to_string()));
    assert_eq!(request.param("missing"), None);
    assert_eq!(request.query("tab"), Some("profile".to_string()));
    assert_eq!(request.query_string(), "tab=profile");
    assert_eq!(
        request.header("accept"),
        Some("application/json".to_string())
    );
    assert_eq!(
        request.header("ACCEPT"),
        Some("application/json".to_string())
    );
    assert_eq!(request.header("missing"), None);
    assert_eq!(request.cookie("theme"), Some("dark".to_string()));
}

/// Verifies request metadata names remain text at the HTTP boundary.
///
/// Inputs:
/// - Route, query, header, and cookie names that look like Vm atom
///   construction functions.
///
/// Output:
/// - Test passes when each lookup returns the associated text value.
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

    assert_eq!(request.param("binary_to_atom"), Some("route".to_string()));
    assert_eq!(request.query("list_to_atom"), Some("query".to_string()));
    assert_eq!(request.header("binary-to-atom"), Some("header".to_string()));
    assert_eq!(request.cookie("list_to_atom"), Some("cookie".to_string()));
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
    let response = json_text(&json_adapter::to_string(&json_adapter::string("ok")), 200);

    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "\"ok\"");
    assert_eq!(response.file_path(), None);
}

/// Verifies serialized JSON responses preserve the supplied body.
///
/// Inputs:
/// - Serialized JSON object text.
///
/// Output:
/// - Test passes when the response uses JSON metadata without changing body
///   bytes.
///
/// Transformation:
/// - Exercises `json_text`, the Rust-backed adapter for
///   `std.http.Response.json_text`.
#[test]
fn json_text_response_uses_json_defaults_without_reparse() {
    let response = json_text("{\"ok\":true}", 202);

    assert_eq!(response.status_code(), 202);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.body(), "{\"ok\":true}");
    assert_eq!(response.file_path(), None);
}

/// Verifies file response construction preserves stream metadata.
///
/// Inputs:
/// - Package-relative file path, status code, and content type override.
///
/// Output:
/// - Test passes when the response exposes file metadata and no body bytes.
///
/// Transformation:
/// - Exercises the Rust-owned `std.http.Response.file` boundary without
///   touching the filesystem or selecting a concrete server implementation.
#[test]
fn file_response_preserves_path_status_and_content_type() {
    let response = file("downloads/report.txt", 206, "text/plain; charset=utf-8");

    assert_eq!(response.status_code(), 206);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "");
    assert_eq!(response.file_path(), Some("downloads/report.txt"));
}

/// Verifies non-VM response adapters reject streaming explicitly.
///
/// Inputs:
/// - One finite chunk and valid response stream policy.
///
/// Output:
/// - Test passes when the adapter returns the stable VM-required error.
///
/// Transformation:
/// - Prevents portable adapters from replacing VM backpressure with implicit
///   concatenation or unbounded buffering.
#[test]
fn stream_response_requires_vm_runtime() {
    let error = stream(&["ok".to_string()], 200, "text/plain", 16_384, 128)
        .expect_err("non-VM streaming must fail");

    assert_eq!(error.code(), "http.response.streaming_requires_vm");
    assert_eq!(error.status(), 500);
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
    let mut response = text("created", 200);
    status(&mut response, 201);
    header(&mut response, "x-terlan", "yes");
    header(&mut response, "Set-Cookie", "session=abc; HttpOnly");

    assert_eq!(response.status_code(), 201);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "created");
    assert_eq!(
        response.headers(),
        &[
            ("x-terlan".to_string(), "yes".to_string()),
            (
                "Set-Cookie".to_string(),
                "session=abc; HttpOnly".to_string()
            )
        ]
    );
}

/// Verifies HTML response construction sets portable defaults.
///
/// Inputs:
/// - Rendered HTML text.
///
/// Output:
/// - Test passes when the response status, content type, and body are stable.
///
/// Transformation:
/// - Stores already-rendered HTML as response body metadata without requiring
///   a concrete server runtime.
#[test]
fn html_response_uses_html_defaults() {
    let response = html("<main>Hello</main>", 200);

    assert_eq!(response.status_code(), 200);
    assert_eq!(response.content_type(), "text/html; charset=utf-8");
    assert_eq!(response.body(), "<main>Hello</main>");
    assert!(response.headers().is_empty());
}

/// Verifies redirect response construction sets portable defaults.
///
/// Inputs:
/// - Redirect location text.
///
/// Output:
/// - Test passes when the response status and `Location` header are stable.
///
/// Transformation:
/// - Encodes a common redirect response without committing to a concrete HTTP
///   framework.
#[test]
fn redirect_response_uses_redirect_defaults() {
    let response = redirect("/login", 302);

    assert_eq!(response.status_code(), 302);
    assert_eq!(response.content_type(), "text/plain; charset=utf-8");
    assert_eq!(response.body(), "");
    assert_eq!(
        response.headers(),
        &[("Location".to_string(), "/login".to_string())]
    );

    let permanent = redirect("/new-login", 301);
    assert_eq!(permanent.status_code(), 301);
    assert_eq!(
        permanent.headers(),
        &[("Location".to_string(), "/new-login".to_string())]
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

/// Verifies portable responses convert into Rust `http` crate responses.
///
/// Inputs:
/// - A Terlan response wrapper with status, content type, body, and headers.
///
/// Output:
/// - Test passes when the standard `http::Response` preserves those fields.
///
/// Transformation:
/// - Exercises the Hyper-ready response boundary before the socket writer is
///   migrated away from manual HTTP text.
#[test]
fn response_converts_to_rust_http_response() {
    let mut response = text("ok", 200);
    header(&mut response, "x-terlan", "yes");
    header(&mut response, "Set-Cookie", "session=abc; Path=/");

    let converted = response
        .to_http_response()
        .expect("valid response should convert");

    assert_eq!(converted.status(), 200);
    assert_eq!(
        converted.headers().get("content-type").unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(converted.headers().get("x-terlan").unwrap(), "yes");
    assert_eq!(
        converted.headers().get("set-cookie").unwrap(),
        "session=abc; Path=/"
    );
    assert_eq!(converted.body(), "ok");
}

/// Verifies Rust `http` responses can return to the portable response shape.
///
/// Inputs:
/// - A standard `http::Response<String>` with content type and extra headers.
///
/// Output:
/// - Test passes when the Terlan response wrapper preserves status, body, and
///   non-content-type headers.
///
/// Transformation:
/// - Locks the bidirectional adapter shape needed by future Hyper service
///   handlers and tests.
#[test]
fn response_converts_from_rust_http_response() {
    let response = ::http::Response::builder()
        .status(201)
        .header(::http::header::CONTENT_TYPE, "application/json")
        .header("x-terlan", "yes")
        .body("{\"ok\":true}".to_string())
        .expect("valid http response");

    let converted = Response::from_http_response(response);

    assert_eq!(converted.status_code(), 201);
    assert_eq!(converted.content_type(), "application/json");
    assert_eq!(converted.body(), "{\"ok\":true}");
    assert_eq!(
        converted.headers(),
        &[("x-terlan".to_string(), "yes".to_string())]
    );
}

/// Verifies invalid response metadata is rejected by the Rust HTTP boundary.
///
/// Inputs:
/// - Terlan response wrappers with invalid status and invalid header metadata.
///
/// Output:
/// - Test passes when conversion returns stable error codes for both failures.
///
/// Transformation:
/// - Uses the maintained `http` crate validation rules instead of preserving
///   custom response-header parsing as a long-term protocol boundary.
#[test]
fn response_to_rust_http_rejects_invalid_status_and_header() {
    let invalid_status = text("bad", 1000);
    let status_error = invalid_status
        .to_http_response()
        .expect_err("invalid status should fail");
    assert_eq!(status_error.code(), "http.response.invalid_status");

    let mut invalid_header = text("bad", 200);
    header(&mut invalid_header, "bad header", "value");
    let header_error = invalid_header
        .to_http_response()
        .expect_err("invalid header should fail");
    assert_eq!(header_error.code(), "http.response.invalid_header");
}
