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
    let joined = join(&base, "main.terl").unwrap_or_else(|_| Path::from_path_buf(PathBuf::new()));
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
