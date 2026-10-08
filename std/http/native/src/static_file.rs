//! Static asset access after package route admission.

use std::path::{Path, PathBuf};

use bytes::Bytes;

use crate::{build_server_response, content_type_for_path, file_response, HttpError};

/// Selects an asset or directory index, checking containment on the final path.
/// Like package file intents, this assumes a host-controlled package tree; it
/// is not a filesystem jail against concurrent untrusted filesystem mutations.
pub fn request_path(root: &Path, request_path: &str) -> Option<PathBuf> {
    let relative = request_path.trim_start_matches('/');
    if relative.is_empty() {
        return file_response::package_relative_path(root, "index.html");
    }
    let path = file_response::package_relative_path(root, relative)?;
    if path.is_dir() || (!path.exists() && path.extension().is_none()) {
        file_response::package_relative_path(root, &format!("{relative}/index.html"))
    } else {
        Some(path)
    }
}

/// Reads an already admitted path and emits the shared static response policy.
/// Source handler file intents use `read_response_file` instead: their failures
/// remain typed errors, and their metadata never gains host response defaults.
/// The optional host transformation is called once on successful reads only;
/// it can inject development tooling without changing the file on disk.
pub fn response(
    path: &Path,
    status: u16,
    content_type: Option<&str>,
    head_only: bool,
    transform: impl FnOnce(&str, Vec<u8>) -> Vec<u8>,
) -> Result<http::Response<Bytes>, HttpError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return build_server_response(
                404,
                "text/plain; charset=utf-8",
                &[],
                Bytes::from_static(b"not found"),
                head_only,
                false,
            );
        }
    };
    let content_type = content_type
        .map(str::to_owned)
        .unwrap_or_else(|| content_type_for_path(path));
    let bytes = transform(&content_type, bytes);
    build_server_response(
        status,
        &content_type,
        &[],
        Bytes::from(bytes),
        head_only,
        false,
    )
}

#[cfg(test)]
#[path = "static_file_test.rs"]
mod tests;
