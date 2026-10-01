//! File content-type selection over the maintained MIME database.

use std::path::Path;

/// Returns a content type for one served package file.
///
/// Inputs:
/// - `path`: response file path.
///
/// Output:
/// - Content-type string.
///
/// Transformation:
/// - Delegates extension detection to `mime_guess`, then applies the runtime's
///   stable UTF-8 charset convention for textual browser artifacts.
pub fn content_type_for_path(path: &Path) -> String {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("map") => return "application/json; charset=utf-8".to_string(),
        Some("woff") => return "font/woff".to_string(),
        Some("woff2") => return "font/woff2".to_string(),
        Some("ttf") => return "font/ttf".to_string(),
        Some("otf") => return "font/otf".to_string(),
        _ => {}
    }
    let essence = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    match essence.as_str() {
        "application/json" | "text/css" | "text/html" | "text/javascript" | "text/markdown"
        | "text/plain" => format!("{essence}; charset=utf-8"),
        _ => essence,
    }
}
