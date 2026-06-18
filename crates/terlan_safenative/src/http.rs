//! HTTP adapter helpers for `std.http`.
//!
//! This module defines the first Rust-owned shape behind Terlan HTTP request
//! and response helpers. It is intentionally small: runtime servers can wrap
//! real sockets and framework requests later, while the compiler can already
//! validate stable operation names and response metadata behavior.

use crate::json::{self, Json};

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

/// Request body wrapper used by the initial HTTP adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    method: String,
    path: String,
    body: String,
}

impl Request {
    /// Builds a request wrapper from body text.
    ///
    /// Inputs:
    /// - `body`: UTF-8 request body text.
    ///
    /// Output:
    /// - `Request` containing the body for later parsing.
    ///
    /// Transformation:
    /// - Stores body text without committing Terlan source to a concrete
    ///   server framework or socket implementation.
    pub fn new(body: impl Into<String>) -> Self {
        Self::from_parts("GET", "/", body)
    }

    /// Builds a request wrapper from server request parts.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    ///
    /// Output:
    /// - `Request` containing stable request metadata and body text.
    ///
    /// Transformation:
    /// - Captures the Rust-native HTTP server request snapshot used by
    ///   adapters before any backend-specific handler bridge consumes it.
    pub fn from_parts(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            body: body.into(),
        }
    }

    /// Returns the request method.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed HTTP method text.
    ///
    /// Transformation:
    /// - Reads the method field without allocation or mutation.
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the request path.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed URL path text without query text.
    ///
    /// Transformation:
    /// - Reads the path field without allocation or mutation.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the request body text.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed UTF-8 body text.
    ///
    /// Transformation:
    /// - Reads the body field without allocation or mutation.
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// Response wrapper used by the initial HTTP adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Response {
    status: i64,
    content_type: String,
    body: String,
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

/// Creates a JSON response.
///
/// Inputs:
/// - `value`: JSON value to render as the response body.
///
/// Output:
/// - Response with status 200 and JSON content type.
///
/// Transformation:
/// - Serializes the JSON value through the existing JSON adapter. JSON values
///   built by the adapter are expected to serialize; any unexpected serializer
///   failure is converted to a stable error response body.
pub fn json(value: &Json) -> Response {
    let body = match json::stringify(value) {
        Ok(text) => text,
        Err(error) => format!("{{\"error\":\"{}\"}}", error.message()),
    };
    Response {
        status: 200,
        content_type: "application/json; charset=utf-8".to_string(),
        body,
        headers: Vec::new(),
    }
}

/// Creates a text response.
///
/// Inputs:
/// - `value`: UTF-8 text body.
///
/// Output:
/// - Response with status 200 and text content type.
///
/// Transformation:
/// - Copies the text into response storage without selecting a concrete server
///   framework.
pub fn text(value: &str) -> Response {
    Response {
        status: 200,
        content_type: "text/plain; charset=utf-8".to_string(),
        body: value.to_string(),
        headers: Vec::new(),
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

#[cfg(test)]
#[path = "http_test.rs"]
mod http_test;
