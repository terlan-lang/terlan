//! Request body admission and upload lifetime, independent of a host executor.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode};
use hyper::body::Body;

use crate::request_body::{
    collect_bounded_body, declared_body_exceeds_limit, spool_bounded_body_to_root, BodyReadError,
    TemporaryBodyFile,
};

/// The host selects storage from the admitted route and supplies configuration.
pub enum BodyStorage<'a> {
    Text,
    File(&'a Path),
}

/// An upload lease attached to request extensions. Clones retain the file;
/// dropping the last request/lease removes it, including after handler failure.
#[derive(Clone, Debug)]
pub struct RequestBodyFile {
    path: String,
    _owner: Arc<TemporaryBodyFile>,
}

impl RequestBodyFile {
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RequestBodyFailure {
    pub status: StatusCode,
    pub message: String,
}

impl RequestBodyFailure {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn from_read(error: BodyReadError, limit: u64) -> Self {
        match error {
            BodyReadError::TooLarge => Self::too_large(limit),
            BodyReadError::Invalid(error) => Self::new(
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {error}"),
            ),
            BodyReadError::Unavailable(error) => Self::new(StatusCode::SERVICE_UNAVAILABLE, error),
        }
    }

    fn too_large(limit: u64) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("request body exceeds {limit} bytes"),
        )
    }
}

/// Rejects a declared overflow before route resolution or body polling.
pub fn validate_declared_length(
    headers: &http::HeaderMap,
    limit: u64,
) -> Result<(), RequestBodyFailure> {
    if declared_body_exceeds_limit(headers, limit) {
        Err(RequestBodyFailure::too_large(limit))
    } else {
        Ok(())
    }
}

/// Preserves request metadata and independently bounds actual body frames.
/// File storage accepts binary data and exposes an empty source text body.
/// Spooling uses synchronous file writes; execution placement remains host-owned.
pub async fn prepare_request<B>(
    request: Request<B>,
    limit: u64,
    storage: BodyStorage<'_>,
) -> Result<Request<String>, RequestBodyFailure>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    validate_declared_length(request.headers(), limit)?;
    let (mut parts, body) = request.into_parts();
    // A replacement body must never inherit a previous body's upload lease.
    parts.extensions.remove::<RequestBodyFile>();
    let text = match storage {
        BodyStorage::Text => {
            let bytes = collect_bounded_body(body, limit)
                .await
                .map_err(|error| RequestBodyFailure::from_read(error, limit))?;
            String::from_utf8(bytes).map_err(|error| {
                RequestBodyFailure::new(
                    StatusCode::BAD_REQUEST,
                    format!("invalid UTF-8 body: {error}"),
                )
            })?
        }
        BodyStorage::File(root) => {
            let owner = spool_bounded_body_to_root(body, limit, root)
                .await
                .map_err(|error| RequestBodyFailure::from_read(error, limit))?;
            let path = owner.path().to_str().ok_or_else(|| {
                RequestBodyFailure::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "temporary request path is not UTF-8",
                )
            })?;
            parts.extensions.insert(RequestBodyFile {
                path: path.to_owned(),
                _owner: Arc::new(owner),
            });
            String::new()
        }
    };
    Ok(Request::from_parts(parts, text))
}

#[cfg(test)]
#[path = "request_ingress_test.rs"]
mod tests;
