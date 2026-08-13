/// Parses declaration-head metadata from one raw shape declaration.
pub(crate) fn raw_shape_signature(raw_kind: &str, text: &str) -> Option<(String, bool, String)> {
    if raw_kind != "shape" {
        return None;
    }

    let trimmed = text.trim();
    let (is_public, after_visibility) =
        if let Some(rest) = trimmed.strip_prefix("pub").and_then(trim_keyword_rest) {
            (true, rest)
        } else {
            (false, trimmed)
        };
    let after_shape = after_visibility
        .strip_prefix("shape")
        .and_then(trim_keyword_rest)?;
    let name = after_shape
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    if name.is_empty() {
        return None;
    }

    let signature = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
    Some((name, is_public, format!("{signature}.")))
}

fn trim_keyword_rest(rest: &str) -> Option<&str> {
    let mut chars = rest.chars();
    chars
        .next()?
        .is_whitespace()
        .then(|| chars.as_str().trim_start())
}
