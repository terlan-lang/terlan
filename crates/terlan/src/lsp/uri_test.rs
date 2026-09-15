//! Protocol identifiers must not silently grant filesystem access.

use super::*;

#[test]
fn virtual_document_identifiers_are_not_filesystem_paths() {
    for source in [
        "untitled:///tmp/document.terl",
        "https://example.test/code.terl",
    ] {
        let uri: Uri = source.parse().unwrap();
        assert_eq!(to_file_path(&uri), None);
    }
}

#[test]
fn file_paths_round_trip_spaces_percent_and_fragment_characters() {
    let path = std::env::temp_dir().join("terlan uri % # unicode-λ.terl");
    let uri = from_file_path(&path).unwrap();
    assert!(uri.fragment().is_none());
    assert!(uri.as_str().contains("%23"));
    assert!(uri.as_str().contains("%25"));
    assert_eq!(to_file_path(&uri), Some(path));
}

#[test]
fn relative_paths_are_not_reinterpreted_against_the_server_directory() {
    assert!(from_file_path("relative/document.terl").is_err());
}

#[cfg(unix)]
#[test]
fn file_paths_preserve_non_utf8_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let path =
        std::env::temp_dir().join(std::ffi::OsString::from_vec(b"terlan-\xff.terl".to_vec()));
    let uri = from_file_path(&path).unwrap();
    assert_eq!(to_file_path(&uri), Some(path));
}
