//! Host response storage and maintained HTTP conversion owned by std.http.

use std::path::Path;

use crate::HttpError;

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

        let mut response = ::http::Response::new(self.body.clone());
        *response.status_mut() = status;
        response
            .headers_mut()
            .insert(::http::header::CONTENT_TYPE, content_type);
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
            response.headers_mut().append(header_name, header_value);
        }

        Ok(response)
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
    ///   NativeBoundary shape used by compiler tests and future runtime bridges.
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

/// Rejects response streaming outside the VM-owned scheduler and transport.
///
/// Inputs:
/// - Ordered response chunks and their explicit HTTP/queue policy.
///
/// Output:
/// - Stable `HttpError` identifying that streaming requires the Terlan VM.
///
/// Transformation:
/// - Keeps `std.http.Response` under one Rust adapter owner while preventing
///   non-VM backends from silently concatenating or unboundedly buffering a
///   source stream.
pub fn stream(
    _chunks: &[String],
    _status_code: i64,
    _content_type: &str,
    _chunk_size: i64,
    _max_pending_writes: i64,
) -> Result<Response, HttpError> {
    Err(HttpError::new(
        "http.response.streaming_requires_vm",
        "response streaming requires the Terlan VM runtime",
        500,
    ))
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
