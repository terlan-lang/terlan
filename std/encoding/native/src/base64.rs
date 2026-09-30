//! Package-owned Base64 codecs delegated to the maintained base64 crate.
//!
//! It preserves backend failures without choosing public Terlan error codes.

use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use base64::Engine;

/// Maintained decoder failures; portable error policy is in Base64.terl.
#[derive(Debug, PartialEq, Eq)]
pub enum DecodeFailure {
    Encoding(base64::DecodeError),
    Utf8(std::string::FromUtf8Error),
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
    encode_bytes(text.as_bytes())
}

/// Encodes an arbitrary byte slice with the standard Base64 alphabet.
///
/// Inputs:
/// - `bytes`: binary payload without a UTF-8 requirement.
///
/// Output:
/// - Base64 text using the standard alphabet and canonical padding.
///
/// Transformation:
/// - Delegates allocation and encoding to the maintained `base64` crate so
///   callers cannot provide an undersized output buffer or observe partial
///   writes.
pub fn encode_bytes(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Decodes standard Base64 text into arbitrary bytes.
pub fn decode_bytes(text: &str) -> Result<Vec<u8>, DecodeFailure> {
    decode_bytes_with_engine(text, STANDARD)
}

/// Decodes standard Base64 text into UTF-8 text.
///
/// Inputs:
/// - `text`: standard Base64 source text.
///
/// Output:
/// - `Ok(String)` when the Base64 payload decodes to valid UTF-8.
/// - `Err(DecodeFailure)` when Base64 decoding or UTF-8 conversion fails.
///
/// Transformation:
/// - Delegates byte decoding to the `base64` crate and validates the decoded
///   bytes as UTF-8 before returning a Terlan string.
pub fn decode(text: &str) -> Result<String, DecodeFailure> {
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
    encode_url_bytes(text.as_bytes())
}

/// Encodes arbitrary bytes with the URL-safe Base64 alphabet.
pub fn encode_url_bytes(bytes: &[u8]) -> String {
    URL_SAFE.encode(bytes)
}

/// Decodes URL-safe Base64 text into UTF-8 text.
///
/// Inputs:
/// - `text`: URL-safe Base64 source text.
///
/// Output:
/// - `Ok(String)` when the Base64 payload decodes to valid UTF-8.
/// - `Err(DecodeFailure)` when Base64 decoding or UTF-8 conversion fails.
///
/// Transformation:
/// - Delegates byte decoding to the `base64` crate and validates the decoded
///   bytes as UTF-8 before returning a Terlan string.
pub fn decode_url(text: &str) -> Result<String, DecodeFailure> {
    decode_with_engine(text, URL_SAFE)
}

/// Decodes URL-safe Base64 text into arbitrary bytes.
pub fn decode_url_bytes(text: &str) -> Result<Vec<u8>, DecodeFailure> {
    decode_bytes_with_engine(text, URL_SAFE)
}

/// Decodes Base64 text with the selected engine and validates UTF-8 output.
///
/// Inputs:
/// - `text`: Base64 source text.
/// - `engine`: selected standard or URL-safe Base64 engine.
///
/// Output:
/// - `Ok(String)` when decoding and UTF-8 validation both succeed.
/// - `Err(DecodeFailure)` identifying the failed maintained decoder.
///
/// Transformation:
/// - Retains backend decode and UTF-8 failures for source-owned error policy.
fn decode_with_engine<E>(text: &str, engine: E) -> Result<String, DecodeFailure>
where
    E: Engine,
{
    let bytes = decode_bytes_with_engine(text, engine)?;
    String::from_utf8(bytes).map_err(DecodeFailure::Utf8)
}

/// Decodes Base64 text with one selected alphabet without UTF-8 validation.
fn decode_bytes_with_engine<E>(text: &str, engine: E) -> Result<Vec<u8>, DecodeFailure>
where
    E: Engine,
{
    engine.decode(text).map_err(DecodeFailure::Encoding)
}

#[cfg(test)]
#[path = "base64_test.rs"]
mod base64_test;
