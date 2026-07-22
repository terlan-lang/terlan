use super::read_file;
use crate::support::test_fs::temp_path;
use std::fs;

/// Locates invalid UTF-8 after a tab-indented source prefix.
#[test]
fn bad_encoding_reports_line_and_character_column() {
    let path = temp_path("bad_encoding", "tab_column").with_extension("terl");
    let mut bytes = b"// coding: utf-8\n// claims UTF-8\n// carries Latin-1\nmodule bad_encoding.\n\npub value(): String ->\n\t    {ok, \"xyz"
        .to_vec();
    let invalid_offset = bytes.len();
    bytes.extend_from_slice(&[0xe5, 0xe4, 0xf6]);
    bytes.extend_from_slice(b"\"}.\n");
    fs::write(&path, bytes).expect("write invalid UTF-8 fixture");

    let error = read_file(path.to_str().expect("UTF-8 fixture path"))
        .expect_err("invalid UTF-8 must be rejected");

    assert_eq!(
        error,
        format!(
            "failed to read {}: Terlan source files must be UTF-8; invalid byte sequence starts at byte {invalid_offset} (line 7, column 15)",
            path.display()
        )
    );
}

/// Keeps invalid-prefix and truncated-sequence boundaries deterministic.
#[test]
fn bad_encoding_reports_adversarial_utf8_boundaries() {
    for (name, bytes, offset, line, column) in [
        ("first_byte", vec![0xff], 0, 1, 1),
        ("truncated", b"module valid.\n\n\xf0\x9f".to_vec(), 15, 3, 1),
    ] {
        let path = temp_path("bad_encoding", name).with_extension("terl");
        fs::write(&path, bytes).expect("write adversarial encoding fixture");
        let error = read_file(path.to_str().expect("UTF-8 fixture path"))
            .expect_err("invalid UTF-8 must be rejected");

        assert!(
            error.ends_with(&format!(
                "invalid byte sequence starts at byte {offset} (line {line}, column {column})"
            )),
            "unexpected encoding diagnostic: {error}"
        );
    }
}
