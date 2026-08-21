/// Injects a static-site base path into generated HTML.
///
/// Inputs:
/// - `html`: generated HTML text.
/// - `base_path`: normalized URL path prefix, usually produced by the caller's
///   CLI or project configuration validation.
///
/// Output:
/// - HTML with a `<base href="...">` tag for every explicitly supplied base.
///
/// Transformation:
/// - Avoids duplicating an existing base tag, inserts after an opening `<head>`
///   tag when present, and otherwise prefixes the fragment so root and project
///   path builds resolve assets identically from nested routes.
pub fn inject_html_base_path(html: &str, base_path: &str) -> String {
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

/// Qualifies fragment-only HTML links against a page's base-relative URL.
///
/// Inputs:
/// - `html`: generated static HTML.
/// - `page_url`: base-relative public URL for the current page.
///
/// Output:
/// - HTML where `href="#fragment"` and single-quoted equivalents include the
///   current page URL before the fragment.
///
/// Transformation:
/// - Prevents an injected HTML `<base>` from redirecting local in-page links to
///   the site root while leaving already-qualified and external links intact.
pub fn qualify_html_fragment_links(html: &str, page_url: &str) -> String {
    let escaped_page_url = crate::terlan_html::escape_html_attr(page_url);
    html.replace("href=\"#", &format!("href=\"{escaped_page_url}#"))
        .replace("href='#", &format!("href='{escaped_page_url}#"))
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
#[cfg(test)]
mod base_path_test;
