/// Escapes text for an HTML attribute value.
///
/// Inputs:
/// - `text`: raw attribute value text.
///
/// Output:
/// - Attribute-safe HTML text.
///
/// Transformation:
/// - Escapes ampersands, quotes, and angle brackets while preserving other
///   characters unchanged.
pub fn escape_html_attr(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escapes text for an HTML text node.
///
/// Inputs:
/// - `text`: raw text node contents.
///
/// Output:
/// - Text-node-safe HTML text.
///
/// Transformation:
/// - Delegates HTML text escaping to `ammonia`, keeping compiler callers out of
///   hand-maintained entity serialization.
pub fn escape_html_text(text: &str) -> String {
    ammonia::clean_text(text)
}

#[cfg(test)]
#[path = "escaping_test.rs"]
mod escaping_test;
