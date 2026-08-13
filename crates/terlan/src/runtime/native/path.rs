//! Lexical path adapter operations for `std.io.Path`.
//!
//! This module is a concrete Rust/NativeBoundary runtime slice for the portable
//! `std.io.Path` contract. It uses Rust `std::path` for target path semantics
//! and intentionally performs no filesystem IO.

use std::ffi::OsString;
use std::path::{Component, Path as StdPath, PathBuf};

/// Lexical path value owned by the NativeBoundary adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    value: PathBuf,
}

impl Path {
    /// Builds a NativeBoundary path value from a Rust path buffer.
    ///
    /// Inputs:
    /// - `value`: backend lexical path value.
    ///
    /// Output:
    /// - A `Path` wrapper suitable for the portable `std.io.Path` API.
    ///
    /// Transformation:
    /// - Wraps the backend representation so callers do not depend on Rust
    ///   path storage directly.
    pub fn from_path_buf(value: PathBuf) -> Self {
        Self { value }
    }

    /// Returns the wrapped Rust path by shared reference.
    ///
    /// Inputs:
    /// - `self`: NativeBoundary path wrapper.
    ///
    /// Output:
    /// - Shared reference to the backend path value.
    ///
    /// Transformation:
    /// - Exposes a read-only view for adapter internals without cloning.
    pub fn as_std_path(&self) -> &StdPath {
        &self.value
    }
}

/// Portable path error returned by NativeBoundary path operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl PathError {
    /// Builds a portable path error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: byte offset when known, or `0` when unavailable.
    ///
    /// Output:
    /// - A `PathError` with stable fields.
    ///
    /// Transformation:
    /// - Converts lexical path failures into one portable shape.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: path error value.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: path error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the path error.
    ///
    /// Inputs:
    /// - `self`: path error value.
    ///
    /// Output:
    /// - Byte offset, or `0` when the backend did not provide a useful offset.
    ///
    /// Transformation:
    /// - Reads the offset field without allocation or mutation.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Parses UTF-8 text into a lexical path value.
///
/// Inputs:
/// - `text`: path source text.
///
/// Output:
/// - `Ok(Path)` when the path is accepted by the NativeBoundary lexical policy.
/// - `Err(PathError)` when the text contains a rejected null byte.
///
/// Transformation:
/// - Converts source text into a Rust `PathBuf` without touching the
///   filesystem.
pub fn from_string(text: &str) -> Result<Path, PathError> {
    reject_null_byte(text)?;
    Ok(Path::from_path_buf(PathBuf::from(text)))
}

/// Renders a lexical path value as UTF-8 text.
///
/// Inputs:
/// - `path`: NativeBoundary path value.
///
/// Output:
/// - Path text rendered with Rust target path semantics.
///
/// Transformation:
/// - Converts the path to a string without touching the filesystem.
pub fn to_string(path: &Path) -> String {
    path.as_std_path().to_string_lossy().into_owned()
}

/// Joins a child path segment to a base path.
///
/// Inputs:
/// - `path`: base path value.
/// - `child`: child path segment text.
///
/// Output:
/// - `Ok(Path)` containing the joined lexical path.
/// - `Err(PathError)` when the child contains a rejected null byte.
///
/// Transformation:
/// - Uses Rust path joining semantics without touching the filesystem.
pub fn join(path: &Path, child: &str) -> Result<Path, PathError> {
    reject_null_byte(child)?;
    Ok(Path::from_path_buf(path.as_std_path().join(child)))
}

/// Returns the final lexical path component.
///
/// Inputs:
/// - `path`: NativeBoundary path value.
///
/// Output:
/// - `Some(String)` when the path has a UTF-8 final component.
/// - `None` when no final component exists.
///
/// Transformation:
/// - Reads path components without touching the filesystem.
pub fn file_name(path: &Path) -> Option<String> {
    path.as_std_path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Returns the final lexical path extension.
///
/// Inputs:
/// - `path`: NativeBoundary path value.
///
/// Output:
/// - `Some(String)` when the final component has a UTF-8 extension.
/// - `None` when no extension exists.
///
/// Transformation:
/// - Reads path components without touching the filesystem.
pub fn extension(path: &Path) -> Option<String> {
    path.as_std_path()
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
}

/// Returns the lexical parent path.
///
/// Inputs:
/// - `path`: NativeBoundary path value.
///
/// Output:
/// - `Some(Path)` when the path has a parent component.
/// - `None` when no parent component exists.
///
/// Transformation:
/// - Reads path components without touching the filesystem.
pub fn parent(path: &Path) -> Option<Path> {
    path.as_std_path()
        .parent()
        .map(|parent| Path::from_path_buf(parent.to_path_buf()))
}

/// Returns whether a lexical path is absolute.
///
/// Inputs:
/// - `path`: NativeBoundary path value.
///
/// Output:
/// - `true` when the path is absolute for the Rust target, otherwise `false`.
///
/// Transformation:
/// - Classifies the path without touching the filesystem.
pub fn is_absolute(path: &Path) -> bool {
    path.as_std_path().is_absolute()
}

/// Lexically normalizes one path without querying the filesystem.
pub fn normalize(path: &Path) -> Path {
    Path::from_path_buf(normalized_path(path.as_std_path()))
}

/// Returns whether normalized `path` begins with normalized `base`.
pub fn starts_with(path: &Path, base: &Path) -> bool {
    normalized_path(path.as_std_path()).starts_with(normalized_path(base.as_std_path()))
}

/// Removes a normalized component prefix without querying the filesystem.
pub fn strip_prefix(path: &Path, base: &Path) -> Option<Path> {
    let normalized = normalized_path(path.as_std_path());
    let normalized_base = normalized_path(base.as_std_path());
    normalized
        .strip_prefix(normalized_base)
        .ok()
        .map(|relative| Path::from_path_buf(relative.to_path_buf()))
}

fn normalized_path(path: &StdPath) -> PathBuf {
    let mut prefix = None::<OsString>;
    let mut rooted = false;
    let mut components = Vec::<OsString>::new();
    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => rooted = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if components
                    .last()
                    .is_some_and(|value| value.as_os_str() != "..")
                {
                    components.pop();
                } else if !rooted {
                    components.push(OsString::from(".."));
                }
            }
            Component::Normal(value) => components.push(value.to_os_string()),
        }
    }
    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if rooted {
        normalized.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for component in components {
        normalized.push(component);
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    normalized
}

/// Rejects source text containing a null byte.
///
/// Inputs:
/// - `text`: path source text or child segment.
///
/// Output:
/// - `Ok(())` when the text contains no null byte.
/// - `Err(PathError)` with stable code `path.null_byte` otherwise.
///
/// Transformation:
/// - Applies Terlan's portable lexical path policy before constructing a path.
fn reject_null_byte(text: &str) -> Result<(), PathError> {
    match text.bytes().position(|byte| byte == 0) {
        Some(offset) => Err(PathError::new(
            "path.null_byte",
            "Path text cannot contain a null byte.",
            offset,
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "path_test.rs"]
#[cfg(test)]
mod path_test;
