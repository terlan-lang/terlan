//! Projection-aware ingress conversion. Source lookup policy remains in Request.terl.

use std::borrow::Cow;

use http::{request::Parts, Extensions, HeaderMap, Method, Uri};

use crate::request_ingress::RequestBodyFile;
use crate::{Request, RequestFieldProjection as Projection, RequestMetadata};

impl Request {
    /// Snapshots a borrowed request for a suspendable invocation. Unobserved
    /// fields are not decoded or copied; the caller retains the upload lease.
    pub fn from_http(
        request: &http::Request<String>,
        params: &[(String, String)],
        projection: Projection,
    ) -> Self {
        snapshot(
            request.method(),
            request.uri(),
            request.headers(),
            request.extensions(),
            Cow::Borrowed(request.body()),
            params,
            projection,
        )
    }

    /// Transfers an owned text body without copying it. The request parts must
    /// remain alive until invocation completion to retain any upload lease.
    pub fn from_http_parts(
        parts: &Parts,
        body: String,
        params: &[(String, String)],
        projection: Projection,
    ) -> Self {
        snapshot(
            &parts.method,
            &parts.uri,
            &parts.headers,
            &parts.extensions,
            Cow::Owned(body),
            params,
            projection,
        )
    }
}

fn snapshot(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    extensions: &Extensions,
    body: Cow<'_, str>,
    params: &[(String, String)],
    projection: Projection,
) -> Request {
    Request::from_parts_with_raw_query_metadata(
        if projection.requires(Projection::METHOD) {
            method.as_str()
        } else {
            ""
        },
        if projection.requires(Projection::PATH) {
            uri.path()
        } else {
            ""
        },
        if projection.requires(Projection::BODY) {
            body.into_owned()
        } else {
            String::new()
        },
        RequestMetadata::from_http(projection, params, uri.query().unwrap_or_default(), headers),
    )
    .with_body_file_path(if projection.requires(Projection::BODY_FILE_PATH) {
        extensions
            .get::<RequestBodyFile>()
            .map_or("", RequestBodyFile::path)
    } else {
        ""
    })
}

#[cfg(test)]
#[path = "request_snapshot_test.rs"]
mod tests;
