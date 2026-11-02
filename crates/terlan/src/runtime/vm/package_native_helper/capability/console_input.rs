//! Generic bounded application stdin, called only inside the owning VM.

use std::io::{self, BufRead};

use crate::terlan_native_boundary::term::NativeBoundaryTerm;

const MAX_LINE_BYTES: usize = 1024 * 1024;

fn failure(code: &str, message: impl Into<String>) -> NativeBoundaryTerm {
    NativeBoundaryTerm::Record {
        name: "Err".to_string(),
        fields: vec![(
            "reason".to_string(),
            NativeBoundaryTerm::Record {
                name: "Error".to_string(),
                fields: vec![
                    (
                        "code".to_string(),
                        NativeBoundaryTerm::Atom(code.to_string()),
                    ),
                    (
                        "message".to_string(),
                        NativeBoundaryTerm::Text(message.into()),
                    ),
                ],
            },
        )],
    }
}

fn read_line(
    reader: &mut impl BufRead,
    maximum: usize,
) -> Result<Option<String>, (&'static str, String)> {
    let mut line = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|error| ("console.io", error.to_string()))?;
        if buffer.is_empty() {
            if line.len() > maximum {
                return Err((
                    "console.line_limit",
                    "Input line exceeds the configured byte bound.".to_string(),
                ));
            }
            return if line.is_empty() {
                Ok(None)
            } else {
                String::from_utf8(line).map(Some).map_err(|_| {
                    (
                        "console.invalid_utf8",
                        "Input line is not UTF-8.".to_string(),
                    )
                })
            };
        }
        let ending = buffer.iter().position(|byte| *byte == b'\n');
        let count = ending.map_or(buffer.len(), |index| index + 1);
        // At most a CRLF terminator may extend the bounded content.
        if line.len().saturating_add(count) > maximum + 2 {
            return Err((
                "console.line_limit",
                "Input line exceeds the configured byte bound.".to_string(),
            ));
        }
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if ending.is_some() {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > maximum {
                return Err((
                    "console.line_limit",
                    "Input line exceeds the configured byte bound.".to_string(),
                ));
            }
            return String::from_utf8(line).map(Some).map_err(|_| {
                (
                    "console.invalid_utf8",
                    "Input line is not UTF-8.".to_string(),
                )
            });
        }
        if line.len() > maximum && line.last() != Some(&b'\r') {
            return Err((
                "console.line_limit",
                "Input line exceeds the configured byte bound.".to_string(),
            ));
        }
    }
}

pub(super) fn read_stdin(maximum: i64) -> NativeBoundaryTerm {
    let Ok(maximum) = usize::try_from(maximum) else {
        return failure(
            "console.invalid_limit",
            "Line bound must be between 1 and 1048576 bytes.",
        );
    };
    if maximum == 0 || maximum > MAX_LINE_BYTES {
        return failure(
            "console.invalid_limit",
            "Line bound must be between 1 and 1048576 bytes.",
        );
    }
    let input = io::stdin();
    match read_line(&mut input.lock(), maximum) {
        Ok(line) => NativeBoundaryTerm::Record {
            name: "Ok".to_string(),
            fields: vec![("value".to_string(), NativeBoundaryTerm::OptionalText(line))],
        },
        Err((code, message)) => failure(code, message),
    }
}

#[cfg(test)]
mod tests {
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
}
