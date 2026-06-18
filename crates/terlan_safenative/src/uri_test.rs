use super::*;

/// Parses a URI fixture for adapter tests.
///
/// Inputs:
/// - `text`: URI source expected to parse.
///
/// Output:
/// - `Some(Uri)` when parsing succeeds.
/// - `None` after a failing assertion when parsing unexpectedly fails.
///
/// Transformation:
/// - Converts a `Result` into an optional test value without unwrap/expect.
fn parsed_uri(text: &str) -> Option<Uri> {
    let result = parse(text);
    assert!(result.is_ok());
    result.ok()
}

/// Validates URI parsing and normalized rendering.
///
/// Inputs:
/// - Full HTTPS URI source text.
///
/// Output:
/// - Test passes when parsing and rendering preserve the normalized URI.
///
/// Transformation:
/// - Exercises the parse/render path over the `url` backend.
#[test]
fn uri_round_trips_normalized_text() {
    let Some(uri) = parsed_uri("https://example.com/docs?q=terlan#intro") else {
        return;
    };
    assert_eq!(to_string(&uri), "https://example.com/docs?q=terlan#intro");
}

/// Validates URI component accessors.
///
/// Inputs:
/// - Full HTTPS URI source text.
///
/// Output:
/// - Test passes when each accessor returns the expected component.
///
/// Transformation:
/// - Reads parsed URI components without reparsing source text.
#[test]
fn uri_component_accessors_return_expected_values() {
    let Some(uri) = parsed_uri("https://example.com/docs?q=terlan#intro") else {
        return;
    };
    assert_eq!(scheme(&uri), "https");
    assert_eq!(host(&uri), Some(String::from("example.com")));
    assert_eq!(path(&uri), "/docs");
    assert_eq!(query(&uri), Some(String::from("q=terlan")));
    assert_eq!(fragment(&uri), Some(String::from("intro")));
}

/// Validates optional URI components.
///
/// Inputs:
/// - URI source text without query or fragment.
///
/// Output:
/// - Test passes when optional accessors return `None`.
///
/// Transformation:
/// - Reads absent parsed components without fabricating defaults.
#[test]
fn absent_optional_components_return_none() {
    let Some(uri) = parsed_uri("https://example.com/docs") else {
        return;
    };
    assert_eq!(query(&uri), None);
    assert_eq!(fragment(&uri), None);
}

/// Validates stable parse error conversion.
///
/// Inputs:
/// - Relative URI text rejected by the selected backend parser.
///
/// Output:
/// - Test passes when parsing returns the stable `uri.parse` code.
///
/// Transformation:
/// - Converts a backend parser error into the portable URI error shape.
#[test]
fn parse_error_uses_stable_error_code() {
    let error = parse("not a uri")
        .err()
        .unwrap_or_else(|| UriError::new("missing", "", 0));
    assert_eq!(error.code(), "uri.parse");
    assert_eq!(error.offset(), 0);
}
