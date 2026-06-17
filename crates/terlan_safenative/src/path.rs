//! Lexical path adapter operations for `std.io.Path`.
//!
//! This module is a concrete Rust/SafeNative runtime slice for the portable
//! `std.io.Path` contract. It uses Rust `std::path` for target path semantics
//! and intentionally performs no filesystem IO.

use std::path::{Path as StdPath, PathBuf};

/// Lexical path value owned by the SafeNative adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    value: PathBuf,
}

impl Path {
    /// Builds a SafeNative path value from a Rust path buffer.
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
    /// - `self`: SafeNative path wrapper.
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

/// Portable path error returned by SafeNative path operations.
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
/// - `Ok(Path)` when the path is accepted by the SafeNative lexical policy.
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
/// - `path`: SafeNative path value.
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
/// - `path`: SafeNative path value.
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
/// - `path`: SafeNative path value.
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
/// - `path`: SafeNative path value.
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
/// - `path`: SafeNative path value.
///
/// Output:
/// - `true` when the path is absolute for the Rust target, otherwise `false`.
///
/// Transformation:
/// - Classifies the path without touching the filesystem.
pub fn is_absolute(path: &Path) -> bool {
    path.as_std_path().is_absolute()
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
mod tests {
    use super::*;

    /// Parses a path fixture for adapter tests.
    ///
    /// Inputs:
    /// - `text`: path source expected to parse.
    ///
    /// Output:
    /// - Parsed `Path`, or an empty path after a failing assertion.
    ///
    /// Transformation:
    /// - Converts a `Result` into a convenient test value without unwrap/expect.
    fn parsed_path(text: &str) -> Path {
        let result = from_string(text);
        assert!(result.is_ok());
        result.unwrap_or_else(|_| Path::from_path_buf(PathBuf::new()))
    }

    /// Validates lexical path parsing and rendering.
    ///
    /// Inputs:
    /// - Relative path text.
    ///
    /// Output:
    /// - Test passes when parsing and rendering preserve the lexical path.
    ///
    /// Transformation:
    /// - Exercises the path construction and string rendering adapter surface.
    #[test]
    fn path_round_trips_relative_text() {
        let path = parsed_path("src/main.terl");
        assert_eq!(to_string(&path), "src/main.terl");
    }

    /// Validates lexical path joining and component accessors.
    ///
    /// Inputs:
    /// - Base path and child segment text.
    ///
    /// Output:
    /// - Test passes when join, file name, extension, and parent are stable.
    ///
    /// Transformation:
    /// - Exercises Rust lexical path operations through the SafeNative wrapper.
    #[test]
    fn join_and_components_are_lexical() {
        let base = parsed_path("src");
        let joined =
            join(&base, "main.terl").unwrap_or_else(|_| Path::from_path_buf(PathBuf::new()));
        let parent = parent(&joined).unwrap_or_else(|| Path::from_path_buf(PathBuf::new()));

        assert_eq!(to_string(&joined), "src/main.terl");
        assert_eq!(file_name(&joined), Some(String::from("main.terl")));
        assert_eq!(extension(&joined), Some(String::from("terl")));
        assert_eq!(to_string(&parent), "src");
    }

    /// Validates absolute path classification.
    ///
    /// Inputs:
    /// - Absolute and relative path text.
    ///
    /// Output:
    /// - Test passes when the Rust target classifies each path correctly.
    ///
    /// Transformation:
    /// - Reads target path classification without touching the filesystem.
    #[test]
    fn absolute_paths_are_classified_without_io() {
        assert!(is_absolute(&parsed_path("/tmp/terlan")));
        assert!(!is_absolute(&parsed_path("tmp/terlan")));
    }

    /// Validates null-byte rejection.
    ///
    /// Inputs:
    /// - Path text containing a null byte.
    ///
    /// Output:
    /// - Test passes when parsing returns the stable path error code and
    ///   offset.
    ///
    /// Transformation:
    /// - Applies the portable lexical path policy before backend construction.
    #[test]
    fn null_byte_uses_stable_error_code_and_offset() {
        let error = from_string("src\0main")
            .err()
            .unwrap_or_else(|| PathError::new("missing", "", 0));
        assert_eq!(error.code(), "path.null_byte");
        assert_eq!(error.offset(), 3);
    }
}
