//! Host file access for source-owned response intents and package manifests.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::{content_type_for_path, HttpError};

/// Resolves a read-only package path, rejecting traversal and escaping symlinks.
/// Missing files retain their path so callers can report absence separately.
/// Package trees must not be mutated by untrusted host processes during access:
/// canonicalization is not a descriptor-relative, race-proof filesystem jail.
pub fn package_relative_path(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.contains('\\') || relative.contains('\0') {
        return None;
    }
    let mut output = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(segment) => output.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let root = root.canonicalize().ok()?;
    match output.canonicalize() {
        Ok(resolved) => resolved.starts_with(root).then_some(resolved),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(output),
        Err(_) => None,
    }
}

/// Reads a file intent after the host explicitly supplies its file capabilities.
/// Absolute paths require a trusted root; an empty root list grants no access.
/// This adapter never reads environment variables or changes source metadata.
pub fn read_response_file(
    package_root: Option<&Path>,
    trusted_roots: &[PathBuf],
    path: &str,
    content_type: String,
) -> Result<(String, Vec<u8>), HttpError> {
    let root = package_root
        .ok_or_else(|| file_error("Response.file requires package file-serving context".into()))?;
    let resolved = package_relative_path(root, path)
        .or_else(|| trusted_file_path(path, trusted_roots))
        .ok_or_else(|| {
            file_error(format!(
                "Response.file path `{path}` is not package-relative or admitted by a trusted file root"
            ))
        })?;
    if !resolved.is_file() {
        return Err(file_error(format!(
            "Response.file path `{path}` does not name a file"
        )));
    }
    let bytes = fs::read(&resolved).map_err(|error| {
        file_error(format!(
            "Response.file path `{path}` cannot be read: {error}"
        ))
    })?;
    let content_type = if content_type.is_empty() {
        content_type_for_path(&resolved)
    } else {
        content_type
    };
    Ok((content_type, bytes))
}

fn trusted_file_path(path: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    let path = path.canonicalize().ok()?;
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| path.starts_with(root))
        .then_some(path)
}

fn file_error(message: String) -> HttpError {
    HttpError::new("http.file", message, 500)
}

#[cfg(test)]
#[path = "file_response_test.rs"]
mod tests;
