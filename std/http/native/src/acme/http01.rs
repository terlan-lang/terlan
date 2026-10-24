//! HTTP-01 method admission and response policy over host project discovery.

use std::path::PathBuf;

use bytes::Bytes;
use http::{Response, StatusCode};

use super::{
    acme_http01_challenge, AcmeHttp01Challenge, ProjectServerTls, ACME_HTTP01_PATH_PREFIX,
};
use crate::{build_server_response, HttpError, ServiceError};

/// Resolves a challenge without inspecting application handlers or VM values.
/// Discovery and cache access happen only for GET/HEAD challenge paths. Errors
/// remain responses, and HEAD preserves the equivalent GET content length.
pub fn response(
    method: &str,
    path: &str,
    discover: impl FnOnce() -> Result<Option<(PathBuf, ProjectServerTls)>, ServiceError>,
) -> Result<Option<Response<Bytes>>, HttpError> {
    if !matches!(method, "GET" | "HEAD") || !path.starts_with(ACME_HTTP01_PATH_PREFIX) {
        return Ok(None);
    }
    let challenge = discover().and_then(|project| {
        acme_http01_challenge(
            project.as_ref().map(|(root, tls)| (root.as_path(), tls)),
            path,
        )
    });
    let (status, body) = match challenge {
        Ok(AcmeHttp01Challenge::NotMatched) => return Ok(None),
        Ok(AcmeHttp01Challenge::Found(body)) => (StatusCode::OK, body),
        Ok(AcmeHttp01Challenge::Missing) => (StatusCode::NOT_FOUND, "not found".into()),
        Ok(AcmeHttp01Challenge::Invalid(message)) => (StatusCode::BAD_REQUEST, message),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    build_server_response(
        status.as_u16(),
        "text/plain; charset=utf-8",
        &[],
        Bytes::from(body),
        method == "HEAD",
        false,
    )
    .map(Some)
}

#[cfg(test)]
mod tests;
