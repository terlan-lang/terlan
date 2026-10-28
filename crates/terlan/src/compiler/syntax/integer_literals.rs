//! Shared integer spelling validation and readable canonical rendering.

/// Removes separators only when every underscore lies between valid digits.
pub(crate) fn normalize_integer(text: &str) -> Option<String> {
    let (sign, unsigned) = text
        .strip_prefix('-')
        .map_or(("", text), |rest| ("-", rest));
    let (prefix, digits, radix) = if let Some(rest) = unsigned.strip_prefix("0b") {
        ("0b", rest, 2)
    } else if let Some(rest) = unsigned.strip_prefix("0o") {
        ("0o", rest, 8)
    } else if let Some(rest) = unsigned.strip_prefix("0x") {
        ("0x", rest, 16)
    } else {
        ("", unsigned, 10)
    };
    let bytes = digits.as_bytes();
    let digit = |byte: u8| byte.is_ascii() && (byte as char).is_digit(radix);
    if bytes.is_empty()
        || bytes.iter().enumerate().any(|(index, byte)| {
            if *byte == b'_' {
                index == 0
                    || index + 1 == bytes.len()
                    || !digit(bytes[index - 1])
                    || !digit(bytes[index + 1])
            } else {
                !digit(*byte)
            }
        })
    {
        return None;
    }
    Some(format!("{sign}{prefix}{}", digits.replace('_', "")))
}

/// Parses validated decimal or radix-prefixed literals within the Int range.
pub(crate) fn parse_integer(text: &str) -> Option<i64> {
    let normalized = normalize_integer(text)?;
    let (sign, unsigned) = normalized
        .strip_prefix('-')
        .map_or(("", normalized.as_str()), |rest| ("-", rest));
    let (digits, radix) = if let Some(rest) = unsigned.strip_prefix("0b") {
        (rest, 2)
    } else if let Some(rest) = unsigned.strip_prefix("0o") {
        (rest, 8)
    } else if let Some(rest) = unsigned.strip_prefix("0x") {
        (rest, 16)
    } else {
        (unsigned, 10)
    };
    i64::from_str_radix(&format!("{sign}{digits}"), radix).ok()
}

/// Formats integer values in decimal groups without overflowing on Int::MIN.
pub(crate) fn format_integer(value: i64) -> String {
    let text = value.to_string();
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |rest| ("-", rest));
    let mut output = String::from(sign);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push('_');
        }
        output.push(digit);
    }
    output
}

/// Groups numeric tokens in type text while preserving strings and comments.
pub(crate) fn format_type_integers(text: &str) -> String {
    let Ok(tokens) = super::lexer::lex(text) else {
        return text.to_owned();
    };
    let mut output = String::new();
    let mut previous = 0;
    for token in tokens {
        if token.kind != super::TokenKind::Int {
            continue;
        }
        let Some(value) = parse_integer(&token.text) else {
            continue;
        };
        output.push_str(&text[previous..token.start]);
        output.push_str(&format_integer(value));
        previous = token.end;
    }
    output.push_str(&text[previous..]);
    output
}

#[cfg(test)]
#[path = "integer_literals_test.rs"]
mod tests;
