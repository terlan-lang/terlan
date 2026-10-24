//! Host request snapshots and maintained HTTP conversion owned by std.http.

use crate::HttpError;

/// Request body wrapper used by the initial HTTP adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    method: String,
    path: String,
    body: String,
    body_file_path: String,
    params: Vec<(String, String)>,
    query_string: String,
    headers: Vec<(String, String)>,
    cookies: Vec<String>,
}

/// Route, query, header, and cookie metadata attached to one HTTP request.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestMetadata {
    /// Decoded route parameters in source-visible order.
    pub params: Vec<(String, String)>,
    /// Raw query text without a leading question mark.
    pub query_string: String,
    /// Decoded request headers in source-visible order.
    pub headers: Vec<(String, String)>,
    /// Raw Cookie headers in wire order; the source accessor owns decoding.
    pub cookies: Vec<String>,
}

/// Owned request metadata transferred to a consuming server or language boundary.
pub struct RequestParts {
    /// HTTP method text.
    pub method: String,
    /// Request path without query text.
    pub path: String,
    /// Text body captured by the server.
    pub body: String,
    /// Server-owned temporary upload path, empty for text bodies.
    pub body_file_path: String,
    /// Decoded route parameters in source-visible order.
    pub params: Vec<(String, String)>,
    /// Raw query text without a leading question mark.
    pub query_string: String,
    /// Decoded headers in source-visible order.
    pub headers: Vec<(String, String)>,
    /// Raw Cookie headers in wire order, including empty or malformed headers.
    pub cookies: Vec<String>,
}

impl Request {
    /// Transfers request storage without applying source query or cookie policy.
    pub fn into_parts(self) -> RequestParts {
        RequestParts {
            method: self.method,
            path: self.path,
            body: self.body,
            body_file_path: self.body_file_path,
            params: self.params,
            query_string: self.query_string,
            headers: self.headers,
            cookies: self.cookies,
        }
    }

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
        Self::from_parts_with_metadata(method, path, body, Vec::new(), String::new(), Vec::new())
    }

    /// Builds a request wrapper from server request parts and decoded metadata.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    /// - `params`: decoded route parameters in source-visible order.
    /// - `query_string`: raw query text without the leading `?`.
    /// - `cookies`: raw Cookie headers in wire order.
    ///
    /// Output:
    /// - `Request` containing stable request metadata, body text, and request
    ///   lookup pairs.
    ///
    /// Transformation:
    /// - Captures server-owned request metadata without selecting a concrete
    ///   web framework or exposing backend route/cookie storage to source
    ///   code.
    pub fn from_parts_with_metadata(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
        params: Vec<(String, String)>,
        query_string: impl Into<String>,
        cookies: Vec<String>,
    ) -> Self {
        Self::from_parts_with_all_metadata(
            method,
            path,
            body,
            params,
            query_string,
            Vec::new(),
            cookies,
        )
    }

    /// Builds a request wrapper from all server request metadata.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    /// - `params`: decoded route parameters in source-visible order.
    /// - `query_string`: raw query text without the leading `?`.
    /// - `headers`: decoded request headers in source-visible order.
    /// - `cookies`: raw Cookie headers in wire order.
    ///
    /// Output:
    /// - `Request` containing stable request metadata, body text, and request
    ///   lookup pairs.
    ///
    /// Transformation:
    /// - Captures server-owned request metadata without selecting a concrete
    ///   web framework or exposing backend route/header/cookie storage to
    ///   source code.
    pub fn from_parts_with_all_metadata(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
        params: Vec<(String, String)>,
        query_string: impl Into<String>,
        headers: Vec<(String, String)>,
        cookies: Vec<String>,
    ) -> Self {
        Self::from_parts_with_raw_query_metadata(
            method,
            path,
            body,
            RequestMetadata {
                params,
                query_string: query_string.into(),
                headers,
                cookies,
            },
        )
    }

    /// Builds a request wrapper from all server request metadata and raw query text.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    /// - `params`: decoded route parameters in source-visible order.
    /// - `query_string`: raw query text without the leading `?`.
    /// - `headers`: decoded request headers in source-visible order.
    /// - `cookies`: raw Cookie headers in wire order.
    ///
    /// Output:
    /// - `Request` containing stable request metadata and lookup pairs.
    ///
    /// Transformation:
    /// - Preserves raw query text for source-selected decoding.
    pub fn from_parts_with_raw_query_metadata(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
        metadata: RequestMetadata,
    ) -> Self {
        let RequestMetadata {
            params,
            query_string,
            headers,
            cookies,
        } = metadata;
        Self {
            method: method.into(),
            path: path.into(),
            body: body.into(),
            body_file_path: String::new(),
            params,
            query_string,
            headers,
            cookies,
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

    /// Attaches the runtime-owned temporary file containing a binary body.
    pub fn with_body_file_path(mut self, path: impl Into<String>) -> Self {
        self.body_file_path = path.into();
        self
    }

    /// Returns the temporary binary-body path, or an empty string for text.
    pub fn body_file_path(&self) -> &str {
        &self.body_file_path
    }

    /// Returns the raw request query string.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed raw query text without the leading `?`.
    ///
    /// Transformation:
    /// - Reads the preserved query string without decoding, splitting, or
    ///   allocation so source handlers can retain exact request metadata.
    pub fn query_string(&self) -> &str {
        &self.query_string
    }

    /// Returns decoded header pairs in request order.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed request header pairs captured by the server bridge.
    ///
    /// Transformation:
    /// - Exposes normalized request metadata to runtime bridges without
    ///   selecting a concrete HTTP server type.
    pub fn header_pairs(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Returns raw Cookie headers for source-selected decoding and jar construction.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed header values in wire order; non-text headers are empty strings.
    ///
    /// Transformation:
    /// - Exposes cookie metadata only inside the HTTP adapter so the facade can
    ///   seed a mutable cookie jar without making request fields public.
    pub fn cookie_headers(&self) -> &[String] {
        &self.cookies
    }

    /// Converts this portable request into a Rust `http` crate request.
    ///
    /// Inputs:
    /// - `self`: request wrapper captured from a server adapter.
    ///
    /// Output:
    /// - `Ok(http::Request<String>)` when method and URI metadata are valid.
    /// - `Err(HttpError)` when the request cannot cross the Rust HTTP boundary.
    ///
    /// Transformation:
    /// - Builds a standards-validated request value for Hyper or another Rust
    ///   HTTP server layer while preserving the UTF-8 body snapshot as the
    ///   request body.
    pub fn to_http_request(&self) -> Result<::http::Request<String>, HttpError> {
        let uri = request_uri_text(self);
        ::http::Request::builder()
            .method(self.method.as_str())
            .uri(uri.as_str())
            .body(self.body.clone())
            .map_err(|error| {
                HttpError::new(
                    "http.request.invalid",
                    format!("request cannot be represented by Rust http crate: {error}"),
                    400,
                )
            })
    }
}

/// Builds the URI text for Rust `http` request conversion.
///
/// Inputs:
/// - `request`: portable request wrapper with path and optional raw query.
///
/// Output:
/// - URI path text with query appended when it was preserved separately.
///
/// Transformation:
/// - Appends preserved raw query text when the path lacks an inline query.
fn request_uri_text(request: &Request) -> String {
    if request.query_string.is_empty() || request.path.contains('?') {
        request.path.clone()
    } else {
        format!("{}?{}", request.path, request.query_string)
    }
}
