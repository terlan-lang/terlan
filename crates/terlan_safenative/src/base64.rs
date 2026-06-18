//! Base64 adapter operations for `std.encoding.Base64`.
//!
//! This module is a concrete Rust/SafeNative runtime slice for the portable
//! `std.encoding.Base64` contract. It delegates encoding and decoding to the
//! `base64` crate and converts decode failures into stable Terlan-facing
//! errors.

use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use base64::Engine;

/// Portable Base64 error returned by SafeNative Base64 operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base64Error {
    code: &'static str,
    message: String,
    offset: usize,
}

impl Base64Error {
    /// Builds a portable Base64 error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `offset`: byte offset when known, or `0` when unavailable.
    ///
    /// Output:
    /// - A `Base64Error` with stable fields.
    ///
    /// Transformation:
    /// - Converts backend decode and UTF-8 failures into one portable shape.
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
    /// - `self`: Base64 error value.
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
    /// - `self`: Base64 error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the Base64 error.
    ///
    /// Inputs:
    /// - `self`: Base64 error value.
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

/// Encodes UTF-8 text with the standard Base64 alphabet and padding.
///
/// Inputs:
/// - `text`: UTF-8 source text.
///
/// Output:
/// - Base64 text using the standard alphabet.
///
/// Transformation:
/// - Delegates byte encoding to the `base64` crate over the input string bytes.
pub fn encode(text: &str) -> String {
    STANDARD.encode(text.as_bytes())
}

/// Decodes standard Base64 text into UTF-8 text.
///
/// Inputs:
/// - `text`: standard Base64 source text.
///
/// Output:
/// - `Ok(String)` when the Base64 payload decodes to valid UTF-8.
/// - `Err(Base64Error)` when Base64 decoding or UTF-8 conversion fails.
///
/// Transformation:
/// - Delegates byte decoding to the `base64` crate and validates the decoded
///   bytes as UTF-8 before returning a Terlan string.
pub fn decode(text: &str) -> Result<String, Base64Error> {
    decode_with_engine(text, STANDARD)
}

/// Encodes UTF-8 text with the URL-safe Base64 alphabet and padding.
///
/// Inputs:
/// - `text`: UTF-8 source text.
///
/// Output:
/// - Base64 text using the URL-safe alphabet.
///
/// Transformation:
/// - Delegates byte encoding to the `base64` crate over the input string bytes.
pub fn encode_url(text: &str) -> String {
    URL_SAFE.encode(text.as_bytes())
}

/// Decodes URL-safe Base64 text into UTF-8 text.
///
/// Inputs:
/// - `text`: URL-safe Base64 source text.
///
/// Output:
/// - `Ok(String)` when the Base64 payload decodes to valid UTF-8.
/// - `Err(Base64Error)` when Base64 decoding or UTF-8 conversion fails.
///
/// Transformation:
/// - Delegates byte decoding to the `base64` crate and validates the decoded
///   bytes as UTF-8 before returning a Terlan string.
pub fn decode_url(text: &str) -> Result<String, Base64Error> {
    decode_with_engine(text, URL_SAFE)
}

/// Decodes Base64 text with the selected engine and validates UTF-8 output.
///
/// Inputs:
/// - `text`: Base64 source text.
/// - `engine`: selected standard or URL-safe Base64 engine.
///
/// Output:
/// - `Ok(String)` when decoding and UTF-8 validation both succeed.
/// - `Err(Base64Error)` with stable code `base64.decode` or `base64.utf8`.
///
/// Transformation:
/// - Converts backend decode and UTF-8 failures into stable portable errors.
fn decode_with_engine<E>(text: &str, engine: E) -> Result<String, Base64Error>
where
    E: Engine,
{
    let bytes = engine
        .decode(text)
        .map_err(|error| Base64Error::new("base64.decode", error.to_string(), 0))?;
    String::from_utf8(bytes).map_err(|error| Base64Error::new("base64.utf8", error.to_string(), 0))
}

#[cfg(test)]
#[path = "base64_test.rs"]
mod base64_test;
