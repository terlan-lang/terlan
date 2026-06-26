use super::support::{diagnostic_display_span, is_valid_sha256_hex, sha256sum_file};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Accepts lowercase SHA-256 hex values.
///
/// Inputs:
/// - One known 64-character lowercase hexadecimal hash.
///
/// Output:
/// - Test passes when the shared validator accepts the hash.
///
/// Transformation:
/// - Exercises the shared checksum shape check without invoking external
///   tools.
#[test]
fn is_valid_sha256_hex_accepts_lowercase_hash() {
    assert!(is_valid_sha256_hex(
        "edeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
    ));
}

/// Rejects non-lowercase or wrong-length SHA-256 values.
///
/// Inputs:
/// - Uppercase, too-short, and non-hex candidate values.
///
/// Output:
/// - Test passes when all malformed values are rejected.
///
/// Transformation:
/// - Keeps checksum validation deterministic before callers compare hashes.
#[test]
fn is_valid_sha256_hex_rejects_invalid_values() {
    assert!(!is_valid_sha256_hex(
        "EDEAaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
    ));
    assert!(!is_valid_sha256_hex("abc"));
    assert!(!is_valid_sha256_hex(
        "zdeaaff3f1774ad2888673770c6d64097e391bc362d7d6fb34982ddf0efd18cb"
    ));
}

/// Computes a known file SHA-256 through the shared helper.
///
/// Inputs:
/// - Temporary file containing `abc`.
///
/// Output:
/// - Test passes when the system hash tool returns the known SHA-256.
///
/// Transformation:
/// - Covers the shared `sha256sum` wrapper used by release and migration
///   validation paths.
#[test]
fn sha256sum_file_hashes_file_contents() {
    let directory = temp_support_dir("sha256sum_file_hashes_file_contents");
    let path = directory.join("input.txt");
    fs::write(&path, "abc").expect("write checksum input");

    assert_eq!(
        sha256sum_file(&path).expect("hash file"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    remove_dir(&directory);
}

/// Narrows unknown-constructor diagnostics to the constructor token.
///
/// Inputs:
/// - Source text containing a function body with `Some(1)`.
/// - A broad fallback span covering the whole function declaration.
///
/// Output:
/// - Test passes when display rendering highlights only `Some`.
///
/// Transformation:
/// - Verifies CLI diagnostics remain readable even when upstream syntax spans
///   are broader than the unresolved constructor token.
#[test]
fn diagnostic_display_span_narrows_unknown_constructor_to_name() {
    let source = "\
module sample.Main.\n\
\n\
pub value(): Dynamic ->\n\
    Some(1).\n\
";
    let fallback_start = source.find("pub value").expect("fallback start");
    let fallback_end = source[fallback_start..]
        .find(".\n")
        .map(|offset| fallback_start + offset + 1)
        .expect("fallback end");
    let expected_start = source.find("Some").expect("constructor start");
    let expected_end = expected_start + "Some".len();

    assert_eq!(
        diagnostic_display_span(
            "type_error",
            "unknown constructor Some / 1",
            source,
            fallback_start,
            fallback_end,
        ),
        (expected_start, expected_end)
    );
}

/// Creates a unique temporary support-test directory.
///
/// Inputs:
/// - `label`: human-readable test label.
///
/// Output:
/// - Path to a newly-created temporary directory.
///
/// Transformation:
/// - Combines process id, timestamp, and label under the OS temp directory so
///   tests do not need an external tempfile crate.
fn temp_support_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "terlan_support_test_{}_{}_{}",
        std::process::id(),
        nanos,
        label
    ));
    fs::create_dir_all(&directory).expect("create temp support directory");
    directory
}

/// Removes a temporary support-test directory.
///
/// Inputs:
/// - `directory`: path created by `temp_support_dir`.
///
/// Output:
/// - Directory is removed or the test fails.
///
/// Transformation:
/// - Cleans up files created by checksum tests.
fn remove_dir(directory: &Path) {
    fs::remove_dir_all(directory).expect("remove temp support directory");
}
