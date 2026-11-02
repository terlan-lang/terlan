use super::*;
use std::io::{BufReader, Cursor};

#[test]
fn preserves_message_boundaries_crlf_and_final_unterminated_line() {
    let mut input = BufReader::with_capacity(2, Cursor::new(b"one\r\ntwo\nlast"));
    assert_eq!(read_line(&mut input, 4).unwrap(), Some("one".into()));
    assert_eq!(read_line(&mut input, 4).unwrap(), Some("two".into()));
    assert_eq!(read_line(&mut input, 4).unwrap(), Some("last".into()));
    assert_eq!(read_line(&mut input, 4).unwrap(), None);
}

#[test]
fn content_bound_does_not_include_crlf() {
    let mut input = BufReader::with_capacity(1, Cursor::new(b"abc\r\n"));
    assert_eq!(read_line(&mut input, 3).unwrap(), Some("abc".into()));
}

#[test]
fn oversized_and_invalid_utf8_input_never_produce_truncated_lines() {
    assert_eq!(
        read_line(&mut Cursor::new(b"abcd\n"), 3).unwrap_err().0,
        "console.line_limit"
    );
    assert_eq!(
        read_line(&mut Cursor::new([255, b'\n']), 3).unwrap_err().0,
        "console.invalid_utf8"
    );
    assert_eq!(
        read_line(&mut Cursor::new(b"abc\r"), 3).unwrap_err().0,
        "console.line_limit"
    );
}

#[test]
fn invalid_bounds_fail_before_accessing_stdin() {
    for bound in [-1, 0, 1048577] {
        assert!(
            matches!(read_stdin(bound), NativeBoundaryTerm::Record { name, .. } if name == "Err")
        );
    }
}
