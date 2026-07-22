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
/// - Exercises the standard Base64 engine through the NativeBoundary wrapper.
#[test]
fn standard_base64_round_trips_text() {
    let encoded = encode("hello Terlan");
    assert_eq!(encoded, "aGVsbG8gVGVybGFu");
    assert_eq!(decode(&encoded), Ok(String::from("hello Terlan")));
}

/// Preserves the canonical RFC 4648 vectors through the byte-oriented path.
#[test]
fn standard_base64_bytes_match_rfc_4648_vectors() {
    let cases = [
        (b"".as_slice(), ""),
        (b"f".as_slice(), "Zg=="),
        (b"fo".as_slice(), "Zm8="),
        (b"foo".as_slice(), "Zm9v"),
        (b"foob".as_slice(), "Zm9vYg=="),
        (b"fooba".as_slice(), "Zm9vYmE="),
        (b"foobar".as_slice(), "Zm9vYmFy"),
    ];

    for (input, expected) in cases {
        assert_eq!(encode_bytes(input), expected);
    }
}

/// Proves arbitrary binary input does not pass through a UTF-8 conversion.
#[test]
fn standard_base64_bytes_cover_every_octet() {
    let input = (u8::MIN..=u8::MAX).collect::<Vec<_>>();
    let encoded = encode_bytes(&input);

    assert_eq!(encoded.len(), 344);
    assert!(encoded.starts_with("AAECAwQFBgcICQoL"));
    assert!(encoded.ends_with("9vf4+fr7/P3+/w=="));
    assert_eq!(STANDARD.decode(encoded), Ok(input));
}

/// Proves the public byte decoder preserves arbitrary non-UTF-8 payloads.
#[test]
fn standard_base64_byte_decoder_round_trips_every_octet() {
    let input = (u8::MIN..=u8::MAX).collect::<Vec<_>>();
    let encoded = encode_bytes(&input);

    assert_eq!(decode_bytes(&encoded), Ok(input));
}

/// Locks padding, output length, and the large-input tail boundary.
#[test]
fn standard_base64_bytes_preserve_padding_for_short_and_large_inputs() {
    for len in (0..=255).chain(2_395..=2_440) {
        let input = (0..len).map(|index| index as u8).collect::<Vec<_>>();
        let encoded = encode_bytes(&input);

        assert_eq!(encoded.len(), ((len + 2) / 3) * 4);
        assert!(encoded
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=') }));
        match len % 3 {
            0 => assert!(!encoded.ends_with('=')),
            1 => assert!(encoded.ends_with("==")),
            2 => assert!(encoded.ends_with('=') && !encoded.ends_with("==")),
            _ => unreachable!(),
        }
        assert_eq!(STANDARD.decode(encoded), Ok(input));
    }

    let input = vec![b'a'; 1_025];
    let encoded = encode_bytes(&input);
    assert_eq!(encoded.len(), 1_368);
    assert!(encoded.starts_with("YWFhYWFhYWFh"));
    assert!(encoded.ends_with("YWE="));
    assert_eq!(STANDARD.decode(encoded), Ok(input));
}

/// Preserves exact alphabets and rejects cross-alphabet decoding.
#[test]
fn base64_byte_modes_are_distinct_and_strict() {
    let input = vec![23, 234, 63, 163, 239, 129, 253, 175, 171];
    let standard = encode_bytes(&input);
    let url_safe = encode_url_bytes(&input);

    assert_eq!(standard, "F+o/o++B/a+r");
    assert_eq!(url_safe, "F-o_o--B_a-r");
    assert_eq!(decode_bytes(&standard), Ok(input.clone()));
    assert_eq!(decode_url_bytes(&url_safe), Ok(input));
    assert!(decode_bytes(&url_safe).is_err());
    assert!(decode_url_bytes(&standard).is_err());
}

/// Rejects missing padding, trailing payload, whitespace, and control bytes.
#[test]
fn base64_byte_decoders_reject_malformed_boundaries() {
    for invalid in [
        "QWxhZGRpbjpvcGVuIHNlc2FtZQ",
        "SGVsbG8gV29ybGQ",
        "dGVzda==a",
        "MDEy MzQ1",
        "\u{13}\u{14}\u{15}\u{16}",
    ] {
        assert!(decode_bytes(invalid).is_err(), "accepted {invalid:?}");
    }
}

/// Exercises the historical 300,000-byte scale boundary without OTP helpers.
#[test]
fn standard_base64_round_trips_large_binary_payload() {
    let input = (0..300_000)
        .map(|index| (index % 256) as u8)
        .collect::<Vec<_>>();
    let encoded = encode_bytes(&input);

    assert_eq!(encoded.len(), 400_000);
    assert_eq!(decode_bytes(&encoded), Ok(input));
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
/// - Exercises the URL-safe Base64 engine through the NativeBoundary wrapper.
#[test]
fn url_safe_base64_round_trips_text() {
    let encoded = encode_url("Terlan: λ");
    assert_eq!(decode_url(&encoded), Ok(String::from("Terlan: λ")));
}

/// Proves URL-safe byte operations preserve alphabet-specific payloads.
#[test]
fn url_safe_base64_bytes_round_trip_non_utf8_payload() {
    let input = vec![251, 255, 239, 0, 128];
    let encoded = encode_url_bytes(&input);

    assert!(!encoded.contains('+') && !encoded.contains('/'));
    assert_eq!(decode_url_bytes(&encoded), Ok(input));
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
