//! Request metadata capture and projection shared by all serving adapters.

use http::HeaderMap;

use crate::{RequestFieldProjection as Projection, RequestMetadata};

/// Preserves header multiplicity and byte order within each name. HeaderName
/// already canonicalizes names; non-UTF-8 values use the existing lossy text API.
pub fn request_header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// Preserves Cookie header boundaries and order. Non-text values and headers
/// without valid text remain empty strings, retaining their source position.
/// The source accessor chooses which header to decode.
pub fn request_cookie_headers(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .map(|value| value.to_str().ok().map(str::to_owned).unwrap_or_default())
        .collect()
}

impl RequestMetadata {
    /// Captures only source-observable metadata. Direct cookie reads and
    /// source-owned jar construction observe the same ordered cookie headers.
    pub fn from_http(
        projection: Projection,
        params: &[(String, String)],
        query: &str,
        headers: &HeaderMap,
    ) -> Self {
        Self {
            params: if projection.requires(Projection::PARAMS) {
                params.to_vec()
            } else {
                Vec::new()
            },
            query_string: if projection.requires(Projection::QUERY_STRING) {
                query.to_owned()
            } else {
                String::new()
            },
            headers: if projection.requires(Projection::HEADERS) {
                request_header_pairs(headers)
            } else {
                Vec::new()
            },
            cookies: if projection.requires(Projection::COOKIES) {
                request_cookie_headers(headers)
            } else {
                Vec::new()
            },
        }
    }
}

#[cfg(test)]
#[path = "request_metadata_test.rs"]
mod tests;
