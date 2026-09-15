//! Filesystem access requires a file URI, not merely an LSP document identifier.

use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types::Uri;

pub(super) fn to_file_path(uri: &Uri) -> Option<PathBuf> {
    let url = url::Url::parse(uri.as_str()).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

pub(super) fn from_file_path(path: impl AsRef<Path>) -> std::io::Result<Uri> {
    let url = url::Url::from_file_path(path).map_err(|()| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "expected an absolute file path",
        )
    })?;
    url.as_str()
        .parse()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
}

#[cfg(test)]
#[path = "uri_test.rs"]
mod tests;
