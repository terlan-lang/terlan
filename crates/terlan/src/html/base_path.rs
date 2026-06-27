/// Injects a static-site base path into generated HTML.
///
/// Inputs:
/// - `html`: generated HTML text.
/// - `base_path`: normalized URL path prefix, usually produced by the caller's
///   CLI or project configuration validation.
///
/// Output:
/// - HTML with a `<base href="...">` tag when `base_path` is not `/`.
///
/// Transformation:
/// - Leaves default-root output unchanged, avoids duplicating an existing base
///   tag, inserts after an opening `<head>` tag when present, and otherwise
///   prefixes the fragment so static route smoke tests and fragment outputs
///   still get deterministic project-prefix behavior.
pub fn inject_html_base_path(html: &str, base_path: &str) -> String {
    if base_path == "/" {
        return html.to_string();
    }

    let lower = html.to_ascii_lowercase();
    if lower.contains("<base ") || lower.contains("<base>") {
        return html.to_string();
    }

    let base_tag = format!(r#"<base href="{base_path}">"#);
    if let Some(insert_at) = find_static_head_open_end(&lower) {
        let mut out = String::with_capacity(html.len() + base_tag.len());
        out.push_str(&html[..insert_at]);
        out.push_str(&base_tag);
        out.push_str(&html[insert_at..]);
        return out;
    }

    format!("{base_tag}{html}")
}

/// Finds the byte offset immediately after the first opening `<head>` tag.
///
/// Inputs:
/// - `lowercase_html`: lowercase copy of generated HTML.
///
/// Output:
/// - Byte offset after the opening `<head...>` tag when present.
///
/// Transformation:
/// - Scans for `<head` while avoiding false positives such as `<header>`, then
///   returns the position after the matching `>`.
fn find_static_head_open_end(lowercase_html: &str) -> Option<usize> {
    for (index, _) in lowercase_html.match_indices("<head") {
        let after_name = &lowercase_html[index + "<head".len()..];
        let is_head_tag = after_name
            .chars()
            .next()
            .is_some_and(|ch| ch == '>' || ch.is_whitespace());
        if !is_head_tag {
            continue;
        }
        let open_end = lowercase_html[index..].find('>')?;
        return Some(index + open_end + 1);
    }

    None
}

#[cfg(test)]
#[path = "base_path_test.rs"]
mod base_path_test;
