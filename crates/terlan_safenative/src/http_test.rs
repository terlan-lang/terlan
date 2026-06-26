use super::*;
use crate::json as json_adapter;
use std::path::Path;

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
    assert_eq!(body_text(&request), r#"{"name":"Ada"}"#);
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
        vec![("id".to_string(), "42".to_string())],
        "tab=profile",
        vec![("tab".to_string(), "profile".to_string())],
        vec![("Accept".to_string(), "application/json".to_string())],
        vec![("theme".to_string(), "dark".to_string())],
    );

    assert_eq!(method(&request), "GET");
    assert_eq!(path(&request), "/users/42");
    assert_eq!(param(&request, "id"), Some("42".to_string()));
    assert_eq!(param(&request, "missing"), None);
    assert_eq!(query(&request, "tab"), Some("profile".to_string()));
    assert_eq!(query_string(&request), "tab=profile");
    assert_eq!(
        request_header(&request, "accept"),
        Some("application/json".to_string())
    );
    assert_eq!(
        request_header(&request, "ACCEPT"),
        Some("application/json".to_string())
    );
    assert_eq!(request_header(&request, "missing"), None);
    assert_eq!(cookie(&request, "theme"), Some("dark".to_string()));
}

/// Verifies request cookies can seed a mutable cookie jar.
///
/// Inputs:
/// - Request metadata containing one incoming cookie.
///
/// Output:
/// - Test passes when the jar can read the incoming cookie and records
///   response cookie mutations.
///
/// Transformation:
/// - Exercises the Rust-side state object behind `Request.cookies()` and
///   `Cookies.Jar` receiver methods without binding a server.
#[test]
fn request_cookies_returns_mutable_cookie_jar() {
    let request = Request::from_parts_with_metadata(
        "GET",
        "/profile",
        "",
        Vec::new(),
        Vec::new(),
        vec![("theme".to_string(), "dark".to_string())],
    );
    let mut jar = cookies(&request);

    assert_eq!(jar.get("theme"), Some("dark".to_string()));
    assert_eq!(jar.get("missing"), None);

    jar.set("session", "abc123", "/", true, false)
        .expect("valid cookie set");
    jar.delete("theme", "/").expect("valid cookie delete");

    assert_eq!(
        jar.mutations(),
        &[
            "session=abc123; HttpOnly; Path=/".to_string(),
            "theme=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string(),
        ]
    );
}

/// Verifies request cookie headers parse through the SafeNative boundary.
///
/// Inputs:
/// - A raw `Cookie` request header with optional whitespace and one malformed
///   segment.
///
/// Output:
/// - Parsed cookie name/value pairs in header order.
///
/// Transformation:
/// - Pins the SafeNative-owned parser boundary used by the BEAM bridge while
///   the actual cookie-pair parsing is delegated to a maintained crate.
#[test]
fn request_cookie_header_parser_splits_request_cookie_pairs() {
    let cookies = parse_request_cookie_header("session=abc; theme = dark; empty; user=Ada");

    assert_eq!(
        cookies,
        vec![
            ("session".to_string(), "abc".to_string()),
            ("theme".to_string(), "dark".to_string()),
            ("user".to_string(), "Ada".to_string()),
        ]
    );
}

/// Verifies invalid cookie jar mutations are rejected without side effects.
///
/// Inputs:
/// - A request-scoped cookie jar.
/// - Cookie set/delete inputs that fail native validation.
///
/// Output:
/// - Test passes when stable cookie error codes are returned and no mutation
///   is recorded.
///
/// Transformation:
/// - Pins the mutable jar boundary to the same conservative validation rules
///   as direct `Set-Cookie` header construction.
#[test]
fn request_cookie_jar_rejects_invalid_mutations_without_recording_them() {
    let request = Request::from_parts_with_metadata(
        "GET",
        "/profile",
        "",
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut jar = cookies(&request);

    let bad_name = jar
        .set("bad name", "abc123", "/", true, false)
        .expect_err("invalid cookie set name");
    let bad_value = jar
        .set("session", "abc;HttpOnly", "/", true, false)
        .expect_err("invalid cookie set value");
    let bad_delete_path = jar
        .delete("session", "relative")
        .expect_err("invalid cookie delete path");

    assert_eq!(bad_name.code(), "http.cookie.invalid_name");
    assert_eq!(bad_value.code(), "http.cookie.invalid_value");
    assert_eq!(bad_delete_path.code(), "http.cookie.invalid_path");
    assert!(jar.mutations().is_empty());
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
    let response = json(&json_adapter::string("ok"), 200);

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

/// Verifies browser runtime asset MIME lookup stays at the HTTP adapter boundary.
///
/// Inputs:
/// - Representative browser, font, image, data, and opaque file paths.
///
/// Output:
/// - Test passes when each path maps to the expected content type.
///
/// Transformation:
/// - Pins the SafeNative MIME boundary backed by `mime_guess`.
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
    set_cookie_header(&mut response, "session=abc; HttpOnly");

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

/// Verifies response composition can consume recorded cookie jar mutations.
///
/// Inputs:
/// - A request-scoped cookie jar with set and delete mutations.
/// - A text response wrapper.
///
/// Output:
/// - Test passes when the response receives `Set-Cookie` headers in mutation
///   order.
///
/// Transformation:
/// - Exercises the Rust-side composition step future HTTP server bridges will
///   use after a Terlan handler returns its response value.
#[test]
fn response_applies_cookie_jar_mutations_in_order() {
    let request = Request::from_parts_with_metadata(
        "GET",
        "/profile",
        "",
        Vec::new(),
        Vec::new(),
        vec![("theme".to_string(), "dark".to_string())],
    );
    let mut jar = cookies(&request);
    jar.set("session", "abc123", "/", true, false)
        .expect("valid cookie set");
    jar.delete("theme", "/").expect("valid cookie delete");

    let mut response = text("ok", 200);
    apply_cookie_mutations(&mut response, &jar);

    assert_eq!(
        response.headers(),
        &[
            (
                "Set-Cookie".to_string(),
                "session=abc123; HttpOnly; Path=/".to_string()
            ),
            (
                "Set-Cookie".to_string(),
                "theme=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_string()
            )
        ]
    );
}

/// Verifies cookie header construction emits the stable first supported shape.
///
/// Inputs:
/// - Cookie name, value, path, and boolean flags.
///
/// Output:
/// - Test passes when the serialized header includes the requested attributes.
///
/// Transformation:
/// - Exercises the Rust-owned cookie serialization boundary without routing a
///   full HTTP request.
#[test]
fn cookie_set_header_serializes_supported_attributes() {
    let header = set_header("session", "abc123", "/account", true, true).expect("valid cookie");

    assert_eq!(header, "session=abc123; HttpOnly; Secure; Path=/account");
}

/// Verifies cookie option serialization covers the typed option surface.
///
/// Inputs:
/// - Cookie name, value, and every currently supported typed option.
///
/// Output:
/// - Test passes when the serialized header includes attributes in stable
///   order.
///
/// Transformation:
/// - Exercises the richer native cookie option boundary before the Terlan
///   source helper grows a mutable cookie jar.
#[test]
fn cookie_set_header_with_options_serializes_full_option_surface() {
    let options = CookieOptions {
        path: "/account".to_string(),
        domain: Some("example.com".to_string()),
        max_age: Some(3600),
        expires: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        http_only: true,
        secure: true,
        same_site: Some(CookieSameSite::Strict),
    };

    let header =
        set_header_with_options("session", "abc123", &options).expect("valid cookie options");

    assert_eq!(
        header,
        "session=abc123; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2026 07:28:00 GMT"
    );
}

/// Verifies every supported SameSite policy serializes predictably.
///
/// Inputs:
/// - Cookie options using `Lax`, `Strict`, and `None` SameSite policies.
///
/// Output:
/// - Test passes when each policy appears with the expected header spelling.
///
/// Transformation:
/// - Locks the native adapter vocabulary used by source-visible cookie option
///   wrappers before richer record-to-native lowering is introduced.
#[test]
fn cookie_set_header_with_options_serializes_same_site_variants() {
    for (policy, expected) in [
        (CookieSameSite::Lax, "SameSite=Lax"),
        (CookieSameSite::Strict, "SameSite=Strict"),
        (CookieSameSite::None, "SameSite=None"),
    ] {
        let mut options = CookieOptions::defaults();
        options.same_site = Some(policy);
        let header =
            set_header_with_options("session", "abc123", &options).expect("valid cookie options");

        assert!(
            header.contains(expected),
            "expected `{expected}` in `{header}`"
        );
    }
}

/// Verifies cookie deletion header construction emits an expiring cookie.
///
/// Inputs:
/// - Cookie name and path.
///
/// Output:
/// - Test passes when deletion metadata is included.
///
/// Transformation:
/// - Exercises the deletion helper that handlers can pass to
///   `Response.set_cookie_header`.
#[test]
fn cookie_delete_header_serializes_expiring_cookie() {
    let header = delete_header("session", "/").expect("valid deletion cookie");

    assert_eq!(
        header,
        "session=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    );
}

/// Verifies invalid cookie names are rejected.
///
/// Inputs:
/// - Cookie names that are empty, reserved, or contain separators.
///
/// Output:
/// - Test passes when all invalid names produce the stable invalid-name code.
///
/// Transformation:
/// - Pins the conservative adapter validation boundary before a richer cookie
///   crate-backed jar is introduced.
#[test]
fn cookie_set_header_rejects_invalid_names() {
    for name in ["", "$Version", "bad name", "bad;name"] {
        let error = set_header(name, "abc", "/", false, false).expect_err("invalid cookie name");

        assert_eq!(error.code(), "http.cookie.invalid_name");
        assert_eq!(error.status(), 400);
    }
}

/// Verifies invalid cookie values and paths are rejected.
///
/// Inputs:
/// - Values and paths that could inject attributes or invalid metadata.
///
/// Output:
/// - Test passes when stable error codes identify the failed field.
///
/// Transformation:
/// - Keeps the first cookie builder intentionally strict at the native adapter
///   boundary.
#[test]
fn cookie_set_header_rejects_invalid_values_and_paths() {
    let value_error =
        set_header("session", "abc;HttpOnly", "/", false, false).expect_err("bad value");
    let path_error = set_header("session", "abc", "relative", false, false).expect_err("bad path");

    assert_eq!(value_error.code(), "http.cookie.invalid_value");
    assert_eq!(path_error.code(), "http.cookie.invalid_path");
}

/// Verifies invalid optional cookie attributes are rejected.
///
/// Inputs:
/// - Cookie options with unsafe domain and expires attribute values.
///
/// Output:
/// - Test passes when both invalid attributes produce the stable option error.
///
/// Transformation:
/// - Pins the validation boundary for future typed cookie option lowering.
#[test]
fn cookie_set_header_with_options_rejects_invalid_optional_attributes() {
    let mut options = CookieOptions::defaults();
    options.domain = Some("bad;domain".to_string());
    let domain_error = set_header_with_options("session", "abc", &options).expect_err("bad domain");

    let mut options = CookieOptions::defaults();
    options.expires = Some("bad\nexpires".to_string());
    let expires_error =
        set_header_with_options("session", "abc", &options).expect_err("bad expires");

    assert_eq!(domain_error.code(), "http.cookie.invalid_attribute");
    assert_eq!(expires_error.code(), "http.cookie.invalid_attribute");
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
        Vec::new(),
        "active=true",
        vec![("active".to_string(), "true".to_string())],
        Vec::new(),
        Vec::new(),
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
    set_cookie_header(&mut response, "session=abc; Path=/");

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
