use super::read_file;
use crate::support::test_fs::temp_path;
use std::fs;

/// Rejects an encoding declaration that attempts to admit Latin-1 source.
#[test]
fn latin1_source_read_rejects_non_utf8_bytes_with_stable_offset() {
    let path = temp_path("latin1_source", "rejected").with_extension("terl");
    let mut bytes =
        b"// coding: latin-1\nmodule latin1_source.\n\npub value(): String -> \"xyz".to_vec();
    let invalid_offset = bytes.len();
    bytes.extend_from_slice(&[0xe5, 0xe4, 0xf6]);
    bytes.extend_from_slice(b"\".\n");
    fs::write(&path, bytes).expect("write Latin-1 source fixture");

    let error = read_file(path.to_str().expect("UTF-8 fixture path"))
        .expect_err("Latin-1 source must be rejected");

    assert_eq!(
        error,
        format!(
            "failed to read {}: Terlan source files must be UTF-8; invalid byte sequence starts at byte {invalid_offset} (line 4, column 28)",
            path.display()
        )
    );
}

/// Accepts the same scalar values when the source is encoded as UTF-8.
#[test]
fn latin1_source_read_accepts_utf8_multibyte_text() {
    let path = temp_path("latin1_source", "utf8").with_extension("terl");
    let source = "module utf8_source.\n\npub value(): String -> \"xyzåäö\".\n";
    fs::write(&path, source).expect("write UTF-8 source fixture");

    assert_eq!(
        read_file(path.to_str().expect("UTF-8 fixture path")).expect("read UTF-8 source"),
        source
    );
}
