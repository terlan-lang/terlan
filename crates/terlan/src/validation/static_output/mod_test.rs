use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::terlan_html::HtmlDiagnostic;

use super::{
    format_html_diagnostics, validate_static_css_output_files, validate_static_html_output,
};

/// Creates a unique temporary path for static-output validation tests.
///
/// Inputs:
/// - `name`: readable test-specific suffix.
///
/// Output:
/// - Path under the system temporary directory.
///
/// Transformation:
/// - Combines process id and nanoseconds to avoid collisions in parallel test
///   runs.
fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-static-output-{name}-{}-{unique}",
        std::process::id()
    ))
}

/// Verifies valid static HTML output passes through the wrapper.
///
/// Inputs:
/// - Rendered HTML text and target output path.
///
/// Output:
/// - `Ok(())` from static output validation.
///
/// Transformation:
/// - Delegates to the HTML validator with template slots disabled.
#[test]
fn validate_static_html_output_accepts_valid_html() {
    validate_static_html_output(
        "<main><h1>Hello</h1><img src=\"/logo.png\"></main>",
        Path::new("public/index.html"),
    )
    .expect("valid static HTML should pass");
}

/// Verifies invalid static HTML diagnostics include the output path.
///
/// Inputs:
/// - Mismatched closing tag and target output path.
///
/// Output:
/// - CLI-ready error string containing path and parser diagnostic.
///
/// Transformation:
/// - Converts structured HTML diagnostics into newline-separated text.
#[test]
fn validate_static_html_output_formats_diagnostics() {
    let message = validate_static_html_output("<main></section>", Path::new("public/bad.html"))
        .expect_err("invalid static HTML should fail");

    assert!(message.contains("public/bad.html"));
    assert!(message.contains("mismatched closing tag"));
}

/// Verifies generated CSS output files are read and validated.
///
/// Inputs:
/// - Temporary CSS output file containing valid CSS.
///
/// Output:
/// - `Ok(())` from static CSS validation.
///
/// Transformation:
/// - Reads the file from disk and delegates source validation to the CSS
///   parser.
#[test]
fn validate_static_css_output_files_accepts_valid_css() {
    let path = temp_path("valid-css.css");
    fs::write(&path, "body { color: red; }\n").expect("write CSS fixture");

    validate_static_css_output_files(std::slice::from_ref(&path))
        .expect("valid static CSS should pass");

    let _ = fs::remove_file(path);
}

/// Verifies unreadable CSS output files produce path-specific diagnostics.
///
/// Inputs:
/// - Missing CSS output path.
///
/// Output:
/// - Error string naming the missing output path.
///
/// Transformation:
/// - Fails before CSS parsing when the generated file cannot be read.
#[test]
fn validate_static_css_output_files_reports_read_errors() {
    let path = temp_path("missing-css.css");

    let message = validate_static_css_output_files(std::slice::from_ref(&path))
        .expect_err("missing CSS output should fail");

    assert!(message.contains("failed to read static CSS output"));
    assert!(message.contains(&path.display().to_string()));
}

/// Verifies invalid CSS diagnostics are converted to CLI text.
///
/// Inputs:
/// - Temporary CSS file containing a parse error.
///
/// Output:
/// - Error string containing the CSS path and parse diagnostic.
///
/// Transformation:
/// - Maps structured CSS diagnostics through the static-output formatter.
#[test]
fn validate_static_css_output_files_formats_css_diagnostics() {
    let path = temp_path("invalid-css.css");
    fs::write(&path, "body { color: '\n'; }").expect("write invalid CSS fixture");

    let message = validate_static_css_output_files(std::slice::from_ref(&path))
        .expect_err("invalid CSS output should fail");

    assert!(message.contains(&path.display().to_string()));
    assert!(message.contains("CSS parse error"));

    let _ = fs::remove_file(path);
}

/// Verifies diagnostic formatting preserves pathless diagnostics.
///
/// Inputs:
/// - One diagnostic with a path and one without a path.
///
/// Output:
/// - Newline-separated message text.
///
/// Transformation:
/// - Formats path-qualified diagnostics as `path: message` and pathless
///   diagnostics as the message alone.
#[test]
fn format_html_diagnostics_formats_pathful_and_pathless_messages() {
    let message = format_html_diagnostics(vec![
        HtmlDiagnostic::new(Some(PathBuf::from("public/page.html")), "bad tag"),
        HtmlDiagnostic::new(None, "global error"),
    ]);

    assert_eq!(message, "public/page.html: bad tag\nglobal error");
}
