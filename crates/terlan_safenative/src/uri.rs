//! URI adapter operations for `std.net.Uri`.
//!
//! This module is a concrete Rust/SafeNative runtime slice for the portable
//! `std.net.Uri` contract. It delegates parsing and rendering to the Rust
//! `url` crate while exposing stable Terlan-facing values and errors.

use url::Url;

/// Parsed URI value owned by the SafeNative adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uri {
    value: Url,
}

impl Uri {
    /// Builds a SafeNative URI value from a Rust `Url`.
    ///
    /// Inputs:
    /// - `value`: backend URI value parsed by the `url` crate.
    ///
    /// Output:
    /// - A `Uri` wrapper suitable for the portable `std.net.Uri` API.
    ///
    /// Transformation:
    /// - Wraps the backend representation so callers do not depend on the
    ///   selected Rust URI crate directly.
    pub fn from_url(value: Url) -> Self {
        Self { value }
    }

    /// Returns the wrapped Rust URL by shared reference.
    ///
    /// Inputs:
    /// - `self`: SafeNative URI wrapper.
    ///
    /// Output:
    /// - Shared reference to the backend URL value.
    ///
    /// Transformation:
    /// - Exposes a read-only view for adapter internals without cloning.
    pub fn as_url(&self) -> &Url {
        &self.value
    }
}

/// Portable URI error returned by SafeNative URI operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UriError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl UriError {
    /// Builds a portable URI error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: byte offset when known, or `0` when unavailable.
    ///
    /// Output:
    /// - A `UriError` with stable fields.
    ///
    /// Transformation:
    /// - Converts backend parser failures into one portable shape.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: URI error value.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: URI error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the URI error.
    ///
    /// Inputs:
    /// - `self`: URI error value.
    ///
    /// Output:
    /// - Byte offset, or `0` when the backend did not provide a useful offset.
    ///
    /// Transformation:
    /// - Reads the offset field without allocation or mutation.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Parses text into a URI value.
///
/// Inputs:
/// - `text`: URI source text.
///
/// Output:
/// - `Ok(Uri)` when the `url` crate accepts the source.
/// - `Err(UriError)` with stable code `uri.parse` when parsing fails.
///
/// Transformation:
/// - Delegates URI parsing to the `url` crate and converts backend diagnostics
///   into the portable Terlan URI error shape.
pub fn parse(text: &str) -> Result<Uri, UriError> {
    Url::parse(text)
        .map(Uri::from_url)
        .map_err(|error| UriError::new("uri.parse", error.to_string(), 0))
}

/// Renders a parsed URI value as normalized text.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - Normalized URI text chosen by the Rust `url` parser.
///
/// Transformation:
/// - Delegates URI rendering to the backend representation.
pub fn to_string(uri: &Uri) -> String {
    uri.as_url().to_string()
}

/// Returns the URI scheme.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - URI scheme text such as `https`.
///
/// Transformation:
/// - Reads the parsed scheme without reparsing source text.
pub fn scheme(uri: &Uri) -> String {
    uri.as_url().scheme().to_owned()
}

/// Returns the URI host when present.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - `Some(String)` when the URI has a host.
/// - `None` when no host is present.
///
/// Transformation:
/// - Reads the parsed host component without exposing backend storage.
pub fn host(uri: &Uri) -> Option<String> {
    uri.as_url().host_str().map(ToOwned::to_owned)
}

/// Returns the URI path.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - URI path component as UTF-8 text.
///
/// Transformation:
/// - Reads the parsed path without reparsing source text.
pub fn path(uri: &Uri) -> String {
    uri.as_url().path().to_owned()
}

/// Returns the URI query when present.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - `Some(String)` when the URI has a query component.
/// - `None` when no query is present.
///
/// Transformation:
/// - Reads the parsed query component without exposing backend storage.
pub fn query(uri: &Uri) -> Option<String> {
    uri.as_url().query().map(ToOwned::to_owned)
}

/// Returns the URI fragment when present.
///
/// Inputs:
/// - `uri`: parsed URI value.
///
/// Output:
/// - `Some(String)` when the URI has a fragment component.
/// - `None` when no fragment is present.
///
/// Transformation:
/// - Reads the parsed fragment component without exposing backend storage.
pub fn fragment(uri: &Uri) -> Option<String> {
    uri.as_url().fragment().map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
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
}
