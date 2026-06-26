//! HTTP adapter helpers for `std.http`.

use std::path::Path;

use crate::json::{self, Json};

mod cookies;
pub use cookies::{parse_request_cookie_header, CookieJar, CookieOptions, CookieSameSite};

/// Portable HTTP adapter error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpError {
    code: &'static str,
    message: String,
    status: i64,
}

impl HttpError {
    /// Builds a portable HTTP adapter error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `status`: HTTP status most closely associated with the failure.
    ///
    /// Output:
    /// - `HttpError` with stable fields.
    ///
    /// Transformation:
    /// - Converts runtime or parser failures into the shared HTTP error shape.
    pub fn new(code: &'static str, message: impl Into<String>, status: i64) -> Self {
        Self {
            code,
            message: message.into(),
            status,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the HTTP status associated with this error.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Numeric HTTP status code.
    ///
    /// Transformation:
    /// - Reads the status field without allocation or mutation.
    pub fn status(&self) -> i64 {
        self.status
    }
}

mod request;
pub use request::Request;

/// Returns a request-scoped cookie jar.
///
/// Inputs:
/// - `request`: request wrapper containing parsed cookies.
///
/// Output:
/// - Cookie jar seeded with request cookies.
///
/// Transformation:
/// - Copies request cookie metadata into a mutable jar resource.
pub fn cookies(request: &Request) -> CookieJar {
    CookieJar::from_pairs(request.cookie_pairs().to_vec())
}

/// Returns an incoming cookie value from a cookie jar.
///
/// Inputs:
/// - `jar`: request-scoped cookie jar.
/// - `name`: cookie name.
///
/// Output:
/// - Optional decoded cookie value.
///
/// Transformation:
/// - Delegates to the cookie jar's incoming-cookie lookup without inspecting
///   recorded response mutations.
pub fn get(jar: &CookieJar, name: &str) -> Option<String> {
    jar.get(name)
}

/// Records a response cookie mutation in a cookie jar.
///
/// Inputs:
/// - `jar`: mutable request-scoped cookie jar.
/// - `name`: cookie name.
/// - `value`: cookie value.
/// - `path`: cookie path attribute.
/// - `http_only`: whether to append `HttpOnly`.
/// - `secure`: whether to append `Secure`.
///
/// Output:
/// - `Ok(())` when the mutation is valid and recorded.
/// - `Err(HttpError)` when cookie validation fails.
///
/// Transformation:
/// - Delegates validation and serialization to the cookie jar so resource and
///   non-resource dispatch use one implementation.
pub fn set(
    jar: &mut CookieJar,
    name: &str,
    value: &str,
    path: &str,
    http_only: bool,
    secure: bool,
) -> Result<(), HttpError> {
    jar.set(name, value, path, http_only, secure)
}

/// Records a response cookie deletion in a cookie jar.
///
/// Inputs:
/// - `jar`: mutable request-scoped cookie jar.
/// - `name`: cookie name.
/// - `path`: cookie path attribute.
///
/// Output:
/// - `Ok(())` when the deletion is valid and recorded.
/// - `Err(HttpError)` when cookie validation fails.
///
/// Transformation:
/// - Delegates deletion-header construction to the cookie jar.
pub fn delete(jar: &mut CookieJar, name: &str, path: &str) -> Result<(), HttpError> {
    jar.delete(name, path)
}

/// Builds a conservative `Set-Cookie` header value.
///
/// Inputs:
/// - `name`: cookie name.
/// - `value`: cookie value.
/// - `path`: cookie path attribute.
/// - `http_only`: whether to append `HttpOnly`.
/// - `secure`: whether to append `Secure`.
///
/// Output:
/// - Serialized `Set-Cookie` header value or an HTTP adapter error.
///
/// Transformation:
/// - Delegates validation and serialization to the cookie helper module while
///   exposing the Rust-backed std function from `std::http`.
pub fn set_header(
    name: &str,
    value: &str,
    path: &str,
    http_only: bool,
    secure: bool,
) -> Result<String, HttpError> {
    cookies::set_header(name, value, path, http_only, secure)
}

/// Builds a `Set-Cookie` header value from typed cookie options.
///
/// Inputs:
/// - `name`: cookie name.
/// - `value`: cookie value.
/// - `options`: typed cookie metadata.
///
/// Output:
/// - Serialized `Set-Cookie` header value or an HTTP adapter error.
///
/// Transformation:
/// - Delegates validation and deterministic attribute serialization to the
///   cookie helper module.
pub fn set_header_with_options(
    name: &str,
    value: &str,
    options: &CookieOptions,
) -> Result<String, HttpError> {
    cookies::set_header_with_options(name, value, options)
}

/// Builds an expiring `Set-Cookie` header value.
///
/// Inputs:
/// - `name`: cookie name.
/// - `path`: cookie path attribute.
///
/// Output:
/// - Serialized deletion header or an HTTP adapter error.
///
/// Transformation:
/// - Delegates cookie deletion serialization to the cookie helper module.
pub fn delete_header(name: &str, path: &str) -> Result<String, HttpError> {
    cookies::delete_header(name, path)
}

/// Response wrapper used by the initial HTTP adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Response {
    status: i64,
    content_type: String,
    body: String,
    file_path: Option<String>,
    headers: Vec<(String, String)>,
}

impl Response {
    /// Builds a response wrapper from server response parts.
    ///
    /// Inputs:
    /// - `status`: numeric HTTP status.
    /// - `content_type`: content type text.
    /// - `body`: UTF-8 response body.
    ///
    /// Output:
    /// - `Response` containing stable metadata and body text.
    ///
    /// Transformation:
    /// - Captures a Rust-native HTTP response snapshot used by server bridge
    ///   code before backend-specific wire conversion.
    pub fn from_parts(
        status: i64,
        content_type: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            status,
            content_type: content_type.into(),
            body: body.into(),
            file_path: None,
            headers: Vec::new(),
        }
    }

    /// Returns the response status.
    ///
    /// Inputs:
    /// - `self`: response wrapper.
    ///
    /// Output:
    /// - Numeric HTTP status code.
    ///
    /// Transformation:
    /// - Reads response metadata without mutation.
    pub fn status_code(&self) -> i64 {
        self.status
    }

    /// Returns the response content type.
    ///
    /// Inputs:
    /// - `self`: response wrapper.
    ///
    /// Output:
    /// - Borrowed content-type text.
    ///
    /// Transformation:
    /// - Reads response metadata without mutation.
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    /// Returns the response body.
    ///
    /// Inputs:
    /// - `self`: response wrapper.
    ///
    /// Output:
    /// - Borrowed body text.
    ///
    /// Transformation:
    /// - Reads response body storage without mutation.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the package-relative file path for a file response.
    ///
    /// Inputs:
    /// - `self`: response wrapper.
    ///
    /// Output:
    /// - `Some(path)` when this response streams a file.
    /// - `None` when this response carries an in-memory body.
    ///
    /// Transformation:
    /// - Exposes file response metadata without reading or validating the
    ///   target file; manifest and server layers own path safety checks.
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Returns response headers.
    ///
    /// Inputs:
    /// - `self`: response wrapper.
    ///
    /// Output:
    /// - Borrowed response header vector.
    ///
    /// Transformation:
    /// - Exposes adapter-owned metadata for tests and future server emission.
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Converts this portable response into a Rust `http` crate response.
    ///
    /// Inputs:
    /// - `self`: response wrapper produced by a Terlan handler or adapter.
    ///
    /// Output:
    /// - `Ok(http::Response<String>)` when status, content type, and extra
    ///   headers are valid according to the Rust HTTP boundary.
    /// - `Err(HttpError)` when response metadata cannot be safely represented.
    ///
    /// Transformation:
    /// - Moves response validation to the maintained `http` crate before the
    ///   server layer serializes the response through Hyper.
    pub fn to_http_response(&self) -> Result<::http::Response<String>, HttpError> {
        let status = u16::try_from(self.status).map_err(|_| {
            HttpError::new(
                "http.response.invalid_status",
                format!(
                    "response status `{}` is outside the HTTP status range",
                    self.status
                ),
                500,
            )
        })?;
        let status = ::http::StatusCode::from_u16(status).map_err(|error| {
            HttpError::new(
                "http.response.invalid_status",
                format!("response status `{}` is invalid: {error}", self.status),
                500,
            )
        })?;
        let content_type = ::http::HeaderValue::from_str(&self.content_type).map_err(|error| {
            HttpError::new(
                "http.response.invalid_content_type",
                format!("response content type cannot be represented as a header: {error}"),
                500,
            )
        })?;

        let mut builder = ::http::Response::builder()
            .status(status)
            .header(::http::header::CONTENT_TYPE, content_type);
        for (name, value) in &self.headers {
            let header_name = ::http::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                HttpError::new(
                    "http.response.invalid_header",
                    format!("response header name `{name}` is invalid: {error}"),
                    500,
                )
            })?;
            let header_value = ::http::HeaderValue::from_str(value).map_err(|error| {
                HttpError::new(
                    "http.response.invalid_header",
                    format!("response header `{name}` value is invalid: {error}"),
                    500,
                )
            })?;
            builder = builder.header(header_name, header_value);
        }

        builder.body(self.body.clone()).map_err(|error| {
            HttpError::new(
                "http.response.invalid",
                format!("response cannot be represented by Rust http crate: {error}"),
                500,
            )
        })
    }

    /// Builds a portable response from a Rust `http` crate response.
    ///
    /// Inputs:
    /// - `response`: Rust HTTP response carrying a UTF-8 body string.
    ///
    /// Output:
    /// - Terlan response wrapper with status, content type, body, and
    ///   non-content-type headers preserved.
    ///
    /// Transformation:
    /// - Converts Hyper/http-adjacent response values back into the portable
    ///   SafeNative shape used by compiler tests and future runtime bridges.
    pub fn from_http_response(response: ::http::Response<String>) -> Self {
        let status = i64::from(response.status().as_u16());
        let content_type = response
            .headers()
            .get(::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let headers = response
            .headers()
            .iter()
            .filter(|(name, _)| *name != ::http::header::CONTENT_TYPE)
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|text| (name.as_str().to_string(), text.to_string()))
            })
            .collect();
        let body = response.into_body();

        Self {
            status,
            content_type,
            body,
            file_path: None,
            headers,
        }
    }
}

/// Parses a request body as JSON.
///
/// Inputs:
/// - `request`: request wrapper carrying UTF-8 body text.
///
/// Output:
/// - `Ok(Json)` when the body parses.
/// - `Err(HttpError)` with status 400 when parsing fails.
///
/// Transformation:
/// - Delegates parsing to the existing JSON adapter and maps parser failures
///   into the HTTP error shape expected by `std.http.Request.body_json`.
pub fn body_json(request: &Request) -> Result<Json, HttpError> {
    json::parse(request.body())
        .map_err(|error| HttpError::new("http.body_json", error.message().to_string(), 400))
}

/// Returns the request body text.
///
/// Inputs:
/// - `request`: request wrapper.
///
/// Output:
/// - Owned UTF-8 body text.
///
/// Transformation:
/// - Copies the stable body snapshot for SafeNative dispatch without parsing
///   or interpreting content type.
pub fn body_text(request: &Request) -> String {
    request.body().to_string()
}

/// Returns the HTTP method from a request wrapper.
///
/// Inputs:
/// - `request`: request wrapper.
///
/// Output:
/// - Owned method text.
///
/// Transformation:
/// - Copies the stable method metadata for SafeNative dispatch.
pub fn method(request: &Request) -> String {
    request.method().to_string()
}

/// Returns the URL path from a request wrapper.
///
/// Inputs:
/// - `request`: request wrapper.
///
/// Output:
/// - Owned URL path text without query text.
///
/// Transformation:
/// - Copies the stable path metadata for SafeNative dispatch.
pub fn path(request: &Request) -> String {
    request.path().to_string()
}

/// Returns a captured route parameter value.
///
/// Inputs:
/// - `request`: request wrapper.
/// - `name`: route parameter name.
///
/// Output:
/// - Optional decoded route parameter value.
///
/// Transformation:
/// - Delegates to the request wrapper's ordered metadata lookup.
pub fn param(request: &Request, name: &str) -> Option<String> {
    request.param(name)
}

/// Returns a decoded query parameter value.
///
/// Inputs:
/// - `request`: request wrapper.
/// - `name`: query parameter name.
///
/// Output:
/// - Optional decoded query parameter value.
///
/// Transformation:
/// - Delegates to the request wrapper's ordered metadata lookup.
pub fn query(request: &Request, name: &str) -> Option<String> {
    request.query(name)
}

/// Returns the raw request query string.
///
/// Inputs:
/// - `request`: request wrapper.
///
/// Output:
/// - Owned raw query text without the leading `?`.
///
/// Transformation:
/// - Copies the preserved query string metadata for SafeNative dispatch
///   without decoding or splitting it.
pub fn query_string(request: &Request) -> String {
    request.query_string().to_string()
}

/// Returns a decoded request header value.
///
/// Inputs:
/// - `request`: request wrapper.
/// - `name`: header name.
///
/// Output:
/// - Optional decoded request header value.
///
/// Transformation:
/// - Delegates to the request wrapper's ordered, case-insensitive header
///   metadata lookup.
pub fn request_header(request: &Request, name: &str) -> Option<String> {
    request.header(name)
}

/// Returns a parsed request cookie value.
///
/// Inputs:
/// - `request`: request wrapper.
/// - `name`: cookie name.
///
/// Output:
/// - Optional decoded request cookie value.
///
/// Transformation:
/// - Delegates to the request wrapper's ordered metadata lookup.
pub fn cookie(request: &Request, name: &str) -> Option<String> {
    request.cookie(name)
}

/// Creates a JSON response.
///
/// Inputs:
/// - `value`: JSON value to render as the response body.
/// - `status_code`: numeric HTTP status.
///
/// Output:
/// - Response with the supplied status and JSON content type.
///
/// Transformation:
/// - Serializes the JSON value through the existing JSON adapter. JSON values
///   built by the adapter are expected to serialize; any unexpected serializer
///   failure is converted to a stable error response body.
pub fn json(value: &Json, status_code: i64) -> Response {
    let body = match json::stringify(value) {
        Ok(text) => text,
        Err(error) => format!("{{\"error\":\"{}\"}}", error.message()),
    };
    Response {
        status: status_code,
        content_type: "application/json; charset=utf-8".to_string(),
        body,
        file_path: None,
        headers: Vec::new(),
    }
}

/// Creates a JSON response from already serialized text.
///
/// Inputs:
/// - `value`: UTF-8 JSON text supplied by trusted handler code.
/// - `status_code`: numeric HTTP status.
///
/// Output:
/// - Response with the supplied status, JSON content type, and unchanged body.
///
/// Transformation:
/// - Stores serialized JSON text directly so handlers can return generated JSON
///   without reparsing it through the `std.data.Json` adapter.
pub fn json_text(value: &str, status_code: i64) -> Response {
    Response {
        status: status_code,
        content_type: "application/json; charset=utf-8".to_string(),
        body: value.to_string(),
        file_path: None,
        headers: Vec::new(),
    }
}

/// Creates a text response.
///
/// Inputs:
/// - `value`: UTF-8 text body.
/// - `status_code`: numeric HTTP status.
///
/// Output:
/// - Response with the supplied status and text content type.
///
/// Transformation:
/// - Copies the text into response storage without selecting a concrete server
///   framework.
pub fn text(value: &str, status_code: i64) -> Response {
    Response {
        status: status_code,
        content_type: "text/plain; charset=utf-8".to_string(),
        body: value.to_string(),
        file_path: None,
        headers: Vec::new(),
    }
}

/// Creates an HTML response.
///
/// Inputs:
/// - `value`: UTF-8 HTML body.
/// - `status_code`: numeric HTTP status.
///
/// Output:
/// - Response with the supplied status and HTML content type.
///
/// Transformation:
/// - Copies rendered HTML into response storage without selecting a concrete
///   server framework or template renderer.
pub fn html(value: &str, status_code: i64) -> Response {
    Response {
        status: status_code,
        content_type: "text/html; charset=utf-8".to_string(),
        body: value.to_string(),
        file_path: None,
        headers: Vec::new(),
    }
}

/// Creates a file response.
///
/// Inputs:
/// - `path`: package-relative file path selected by the handler.
/// - `status_code`: numeric HTTP status.
/// - `content_type`: optional content type override; an empty string lets the
///   server infer the file content type.
///
/// Output:
/// - Response with file metadata and no in-memory body.
///
/// Transformation:
/// - Stores file response metadata without touching the filesystem so compile
///   and serve manifest validation remain the safety boundary for path checks.
pub fn file(path: &str, status_code: i64, content_type: &str) -> Response {
    Response {
        status: status_code,
        content_type: content_type.to_string(),
        body: String::new(),
        file_path: Some(path.to_string()),
        headers: Vec::new(),
    }
}

/// Returns a content type for one served package file.
///
/// Inputs:
/// - `path`: response file path.
///
/// Output:
/// - Content-type string.
///
/// Transformation:
/// - Delegates extension detection to `mime_guess`, then applies the runtime's
///   stable UTF-8 charset convention for textual browser artifacts.
pub fn content_type_for_path(path: &Path) -> String {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("map") => return "application/json; charset=utf-8".to_string(),
        Some("woff") => return "font/woff".to_string(),
        Some("woff2") => return "font/woff2".to_string(),
        Some("ttf") => return "font/ttf".to_string(),
        Some("otf") => return "font/otf".to_string(),
        _ => {}
    }
    let essence = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    match essence.as_str() {
        "application/json" | "text/css" | "text/html" | "text/javascript" | "text/markdown"
        | "text/plain" => format!("{essence}; charset=utf-8"),
        _ => essence,
    }
}

/// Creates a redirect response.
///
/// Inputs:
/// - `location`: target redirect location.
/// - `status_code`: numeric HTTP redirect status.
///
/// Output:
/// - Response with the supplied status, an empty text body, and a `Location`
///   header.
///
/// Transformation:
/// - Encodes the common redirect response shape as portable metadata while
///   leaving URL validation to higher-level routing/configuration layers.
pub fn redirect(location: &str, status_code: i64) -> Response {
    Response {
        status: status_code,
        content_type: "text/plain; charset=utf-8".to_string(),
        body: String::new(),
        file_path: None,
        headers: vec![("Location".to_string(), location.to_string())],
    }
}

/// Sets the response status.
///
/// Inputs:
/// - `response`: mutable response wrapper.
/// - `code`: numeric HTTP status code.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Updates response metadata in place for mutable receiver lowering.
pub fn status(response: &mut Response, code: i64) {
    response.status = code;
}

/// Sets or appends a response header.
///
/// Inputs:
/// - `response`: mutable response wrapper.
/// - `name`: UTF-8 header name.
/// - `value`: UTF-8 header value.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Stores header metadata for later server emission.
pub fn header(response: &mut Response, name: &str, value: &str) {
    response.headers.push((name.to_string(), value.to_string()));
}

/// Appends a `Set-Cookie` response header value.
///
/// Inputs:
/// - `response`: mutable response wrapper.
/// - `value`: complete `Set-Cookie` header value.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Stores cookie header metadata through the same response header path used
///   by direct mutable receiver lowering.
pub fn set_cookie_header(response: &mut Response, value: &str) {
    header(response, "Set-Cookie", value);
}

/// Applies recorded cookie jar mutations to a response.
///
/// Inputs:
/// - `response`: mutable response wrapper that will be emitted by a server
///   adapter.
/// - `jar`: request-scoped cookie jar mutated by handler code.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Appends one `Set-Cookie` response header for each recorded jar mutation
///   while preserving the mutation order chosen by handler execution.
pub fn apply_cookie_mutations(response: &mut Response, jar: &CookieJar) {
    for mutation in jar.mutations() {
        set_cookie_header(response, mutation);
    }
}

#[cfg(test)]
#[path = "http_test.rs"]
mod http_test;
