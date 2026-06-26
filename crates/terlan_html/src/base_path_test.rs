use super::*;

/// Verifies root base paths leave generated HTML unchanged.
///
/// Inputs:
/// - HTML containing a normal document head.
/// - Root base path `/`.
///
/// Output:
/// - Original HTML without a `<base>` tag.
///
/// Transformation:
/// - Confirms root-hosted static sites avoid unnecessary document mutation.
#[test]
fn inject_html_base_path_leaves_root_output_unchanged() {
    let html = "<!doctype html><head><title>Home</title></head><body></body>";

    assert_eq!(inject_html_base_path(html, "/"), html);
}

/// Verifies base paths are inserted inside an opening head tag.
///
/// Inputs:
/// - HTML document with a `<head>` element.
/// - Normalized project base path.
///
/// Output:
/// - HTML with `<base href="/docs/">` immediately after the opening head tag.
///
/// Transformation:
/// - Pins the GitHub-Pages-style project-prefix insertion point.
#[test]
fn inject_html_base_path_inserts_inside_head() {
    let html = "<!doctype html><html><head><meta charset=\"utf-8\"></head><body></body></html>";

    assert_eq!(
        inject_html_base_path(html, "/docs/"),
        "<!doctype html><html><head><base href=\"/docs/\"><meta charset=\"utf-8\"></head><body></body></html>"
    );
}

/// Verifies existing base tags are not duplicated.
///
/// Inputs:
/// - HTML document already containing a `<base>` tag.
/// - Normalized project base path.
///
/// Output:
/// - Original HTML unchanged.
///
/// Transformation:
/// - Preserves caller-provided base configuration instead of overwriting it.
#[test]
fn inject_html_base_path_preserves_existing_base() {
    let html = "<head><base href=\"/existing/\"><title>Home</title></head>";

    assert_eq!(inject_html_base_path(html, "/docs/"), html);
}

/// Verifies fragments receive a deterministic base prefix.
///
/// Inputs:
/// - HTML fragment without a document head.
/// - Normalized project base path.
///
/// Output:
/// - Fragment prefixed with the generated base tag.
///
/// Transformation:
/// - Keeps static route smoke tests and fragment outputs deterministic when no
///   full document shell is present.
#[test]
fn inject_html_base_path_prefixes_fragments_without_head() {
    assert_eq!(
        inject_html_base_path("<main>Home</main>", "/docs/"),
        "<base href=\"/docs/\"><main>Home</main>"
    );
}
