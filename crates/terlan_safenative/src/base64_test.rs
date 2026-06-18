use super::*;

/// Validates standard Base64 encode/decode over UTF-8 text.
///
/// Inputs:
/// - ASCII text fixture.
///
/// Output:
/// - Test passes when encoding and decoding produce stable text.
///
/// Transformation:
/// - Exercises the standard Base64 engine through the SafeNative wrapper.
#[test]
fn standard_base64_round_trips_text() {
    let encoded = encode("hello Terlan");
    assert_eq!(encoded, "aGVsbG8gVGVybGFu");
    assert_eq!(decode(&encoded), Ok(String::from("hello Terlan")));
}

/// Validates URL-safe Base64 encode/decode over UTF-8 text.
///
/// Inputs:
/// - Unicode text fixture.
///
/// Output:
/// - Test passes when URL-safe encoding and decoding preserve the text.
///
/// Transformation:
/// - Exercises the URL-safe Base64 engine through the SafeNative wrapper.
#[test]
fn url_safe_base64_round_trips_text() {
    let encoded = encode_url("Terlan: λ");
    assert_eq!(decode_url(&encoded), Ok(String::from("Terlan: λ")));
}

/// Validates decode error conversion.
///
/// Inputs:
/// - Invalid Base64 source text.
///
/// Output:
/// - Test passes when decoding returns the stable decode error code.
///
/// Transformation:
/// - Converts a backend decode failure into the portable Base64 error
///   shape.
#[test]
fn invalid_base64_uses_stable_error_code() {
    let error = decode("not base64!")
        .err()
        .unwrap_or_else(|| Base64Error::new("missing", "", 0));
    assert_eq!(error.code(), "base64.decode");
    assert_eq!(error.offset(), 0);
}

/// Validates UTF-8 error conversion after successful byte decoding.
///
/// Inputs:
/// - Base64 text for invalid UTF-8 bytes.
///
/// Output:
/// - Test passes when decoding returns the stable UTF-8 error code.
///
/// Transformation:
/// - Converts decoded non-UTF-8 bytes into the portable Base64 error shape.
#[test]
fn invalid_utf8_payload_uses_stable_error_code() {
    let error = decode("//4=")
        .err()
        .unwrap_or_else(|| Base64Error::new("missing", "", 0));
    assert_eq!(error.code(), "base64.utf8");
}
