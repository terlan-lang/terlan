//! Bounded request-body transport. Callers own limits, placement, and scheduling.

use std::io::Write;
use std::path::Path;

use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::Body;
use tempfile::TempPath;

#[derive(Debug, Eq, PartialEq)]
pub enum BodyReadError {
    TooLarge,
    Invalid(String),
    Unavailable(String),
}

/// Owns a completed upload until the request finishes. Dropping the owner removes
/// the file, including when a handler fails or its future is cancelled.
#[derive(Debug)]
pub struct TemporaryBodyFile(TempPath);

impl TemporaryBodyFile {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Early size rejection only; the maintained HTTP parser validates framing.
/// Actual data frames are independently counted even without Content-Length.
pub fn declared_body_exceeds_limit(headers: &http::HeaderMap, limit: u64) -> bool {
    headers
        .get_all(http::header::CONTENT_LENGTH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.trim().parse::<u64>().ok())
        .any(|length| length > limit)
}

pub async fn collect_bounded_body<B>(body: B, limit: u64) -> Result<Vec<u8>, BodyReadError>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let mut bytes = Vec::new();
    copy_bounded_body(body, limit, &mut bytes).await?;
    Ok(bytes)
}

/// Spools binary frames under an explicit directory without environment access.
/// File writes are synchronous; the host must supply an appropriate execution
/// context. This adapter does not create an executor or background worker.
pub async fn spool_bounded_body_to_root<B>(
    body: B,
    limit: u64,
    root: &Path,
) -> Result<TemporaryBodyFile, BodyReadError>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    if !root.is_absolute() {
        return Err(BodyReadError::Unavailable(
            "TERLAN_SERVE_UPLOAD_ROOT must be absolute".into(),
        ));
    }
    std::fs::create_dir_all(root).map_err(|error| {
        BodyReadError::Unavailable(format!("cannot create upload root: {error}"))
    })?;
    let root = std::fs::canonicalize(root).map_err(|error| {
        BodyReadError::Unavailable(format!("cannot resolve upload root: {error}"))
    })?;
    let mut file = tempfile::Builder::new()
        .prefix("terlan-request-body-")
        .suffix(".upload")
        .tempfile_in(root)
        .map_err(|error| {
            BodyReadError::Unavailable(format!("cannot create request body file: {error}"))
        })?;
    copy_bounded_body(body, limit, &mut file).await?;
    Ok(TemporaryBodyFile(file.into_temp_path()))
}

async fn copy_bounded_body<B, W>(body: B, limit: u64, writer: &mut W) -> Result<(), BodyReadError>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
    W: Write,
{
    let mut body = body;
    let mut written = 0_u64;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| BodyReadError::Invalid(error.to_string()))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        written = written
            .checked_add(data.len() as u64)
            .filter(|length| *length <= limit)
            .ok_or(BodyReadError::TooLarge)?;
        writer
            .write_all(&data)
            .map_err(|error| BodyReadError::Unavailable(format!("cannot spool body: {error}")))?;
    }
    writer
        .flush()
        .map_err(|error| BodyReadError::Unavailable(format!("cannot flush body: {error}")))
}

#[cfg(test)]
#[path = "request_body_test.rs"]
mod tests;
