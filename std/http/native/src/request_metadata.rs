//! Request metadata decoding and projection shared by all serving adapters.

use http::HeaderMap;

use crate::{RequestFieldProjection as Projection, RequestMetadata};

pub use terlan_net_native::query_pairs;

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

/// Keeps the existing request contract: parse the first Cookie header when it
/// is textual; malformed cookie pairs are handled by the maintained cookie codec.
pub fn request_cookie_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(crate::parse_request_cookie_header)
        .unwrap_or_default()
}

impl RequestMetadata {
    /// Decodes only source-observable metadata. Direct cookie reads and
    /// source-owned jar construction observe the same incoming cookie pairs.
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
            query: if projection.requires(Projection::QUERY) {
                query_pairs(query)
            } else {
                Vec::new()
            },
            headers: if projection.requires(Projection::HEADERS) {
                request_header_pairs(headers)
            } else {
                Vec::new()
            },
            cookies: if projection.requires(Projection::COOKIES) {
                request_cookie_pairs(headers)
            } else {
                Vec::new()
            },
        }
    }
}

#[cfg(test)]
#[path = "request_metadata_test.rs"]
mod tests;
