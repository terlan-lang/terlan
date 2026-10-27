//! Independent implementation of the normative quoted-string lexical mode.

pub(super) fn quoted_end(source: &str) -> Option<usize> {
    quoted(source.as_bytes(), 0, 0)
}

fn quoted(bytes: &[u8], start: usize, nesting: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') || nesting >= 128 {
        return None;
    }
    let mut offset = start + 1;
    while let Some(&byte) = bytes.get(offset) {
        match byte {
            b'"' => return Some(offset + 1),
            b'\\' => offset += 2,
            b'$' if bytes.get(offset + 1) == Some(&b'{') => {
                offset = island(bytes, offset + 2, nesting + 1)?;
            }
            _ => offset += 1,
        }
    }
    None
}

fn island(bytes: &[u8], mut offset: usize, nesting: usize) -> Option<usize> {
    let mut braces = 1;
    while let Some(&byte) = bytes.get(offset) {
        if byte == b'"' {
            offset = quoted(bytes, offset, nesting)?;
        } else if bytes.get(offset..offset + 2) == Some(b"/*") {
            offset += 2;
            while bytes.get(offset..offset + 2) != Some(b"*/") {
                bytes.get(offset)?;
                offset += 1;
            }
            offset += 2;
        } else if byte == b'%' || bytes.get(offset..offset + 2) == Some(b"//") {
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
        } else {
            offset += 1;
            if byte == b'{' {
                braces += 1;
            }
            if byte == b'}' {
                braces -= 1;
                if braces == 0 {
                    return Some(offset);
                }
            }
        }
    }
    None
}
