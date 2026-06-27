use super::HttpError;

/// Request body wrapper used by the initial HTTP adapter contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    method: String,
    path: String,
    body: String,
    params: Vec<(String, String)>,
    query_string: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    cookies: Vec<(String, String)>,
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
        Self::from_parts_with_metadata(method, path, body, Vec::new(), Vec::new(), Vec::new())
    }

    /// Builds a request wrapper from server request parts and decoded metadata.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    /// - `params`: decoded route parameters in source-visible order.
    /// - `query`: decoded query parameters in source-visible order.
    /// - `cookies`: decoded request cookies in source-visible order.
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
        query: Vec<(String, String)>,
        cookies: Vec<(String, String)>,
    ) -> Self {
        Self::from_parts_with_all_metadata(method, path, body, params, query, Vec::new(), cookies)
    }

    /// Builds a request wrapper from all server request metadata.
    ///
    /// Inputs:
    /// - `method`: HTTP method text.
    /// - `path`: URL path without query text.
    /// - `body`: UTF-8 request body text.
    /// - `params`: decoded route parameters in source-visible order.
    /// - `query`: decoded query parameters in source-visible order.
    /// - `headers`: decoded request headers in source-visible order.
    /// - `cookies`: decoded request cookies in source-visible order.
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
        query: Vec<(String, String)>,
        headers: Vec<(String, String)>,
        cookies: Vec<(String, String)>,
    ) -> Self {
        Self::from_parts_with_raw_query_metadata(
            method, path, body, params, "", query, headers, cookies,
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
    /// - `query`: decoded query parameters in source-visible order.
    /// - `headers`: decoded request headers in source-visible order.
    /// - `cookies`: decoded request cookies in source-visible order.
    ///
    /// Output:
    /// - `Request` containing stable request metadata and lookup pairs.
    ///
    /// Transformation:
    /// - Preserves raw query text separately from decoded query pairs.
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts_with_raw_query_metadata(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
        params: Vec<(String, String)>,
        query_string: impl Into<String>,
        query: Vec<(String, String)>,
        headers: Vec<(String, String)>,
        cookies: Vec<(String, String)>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            body: body.into(),
            params,
            query_string: query_string.into(),
            query,
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

    /// Returns the first decoded route parameter value for a name.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    /// - `name`: route parameter name.
    ///
    /// Output:
    /// - `Some(value)` when a matching route parameter exists.
    /// - `None` when the request route did not capture the name.
    ///
    /// Transformation:
    /// - Searches captured route params in declaration order and clones the
    ///   first matching value into a portable optional string.
    pub fn param(&self, name: &str) -> Option<String> {
        find_named_value(&self.params, name)
    }

    /// Returns the first decoded query parameter value for a name.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    /// - `name`: query parameter name.
    ///
    /// Output:
    /// - `Some(value)` when a matching query parameter exists.
    /// - `None` when the query string did not include the name.
    ///
    /// Transformation:
    /// - Searches decoded query pairs in request order and clones the first
    ///   matching value into a portable optional string.
    pub fn query(&self, name: &str) -> Option<String> {
        find_named_value(&self.query, name)
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

    /// Returns the first decoded request header value for a name.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    /// - `name`: header name.
    ///
    /// Output:
    /// - `Some(value)` when a matching request header exists.
    /// - `None` when the request did not include the header.
    ///
    /// Transformation:
    /// - Searches decoded header pairs in request order with ASCII
    ///   case-insensitive key comparison and clones the first matching value
    ///   into a portable optional string.
    pub fn header(&self, name: &str) -> Option<String> {
        find_header_value(&self.headers, name)
    }

    /// Returns the first decoded request cookie value for a name.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    /// - `name`: cookie name.
    ///
    /// Output:
    /// - `Some(value)` when a matching request cookie exists.
    /// - `None` when the request did not include the cookie.
    ///
    /// Transformation:
    /// - Searches parsed request cookies in header order and clones the first
    ///   matching value into a portable optional string.
    pub fn cookie(&self, name: &str) -> Option<String> {
        find_named_value(&self.cookies, name)
    }

    /// Returns decoded request cookie pairs for runtime jar construction.
    ///
    /// Inputs:
    /// - `self`: request wrapper.
    ///
    /// Output:
    /// - Borrowed parsed cookie pairs in request order.
    ///
    /// Transformation:
    /// - Exposes cookie metadata only inside the HTTP adapter so the facade can
    ///   seed a mutable cookie jar without making request fields public.
    pub(super) fn cookie_pairs(&self) -> &[(String, String)] {
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

/// Finds the first matching value in request metadata pairs.
///
/// Inputs:
/// - `pairs`: ordered request metadata key/value pairs.
/// - `name`: requested key.
///
/// Output:
/// - Cloned first matching value when present.
///
/// Transformation:
/// - Preserves repeated metadata behavior by choosing the first decoded pair.
fn find_named_value(pairs: &[(String, String)], name: &str) -> Option<String> {
    pairs
        .iter()
        .find_map(|(key, value)| (key == name).then(|| value.clone()))
}

/// Finds the first matching value in request header pairs.
///
/// Inputs:
/// - `pairs`: ordered request header key/value pairs.
/// - `name`: requested header name.
///
/// Output:
/// - Cloned first matching value when present.
///
/// Transformation:
/// - Compares header names with ASCII case-insensitive equality.
fn find_header_value(pairs: &[(String, String)], name: &str) -> Option<String> {
    pairs
        .iter()
        .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then(|| value.clone()))
}
