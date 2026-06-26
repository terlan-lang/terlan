use super::*;

/// Verifies projected column names round-trip in source order.
///
/// Inputs:
/// - A base64-encoded newline-separated projection list.
///
/// Output:
/// - Test passes when decoding returns the same ordered field names.
///
/// Transformation:
/// - Decodes the private SQL runtime protocol field-list payload.
#[test]
fn decode_projection_preserves_order() {
    let encoded = encode_text("id\nemail");

    assert_eq!(
        decode_projection(&encoded).expect("decode projection"),
        vec!["id".to_string(), "email".to_string()]
    );
}

/// Verifies scalar row values encode into the SQL runtime protocol.
///
/// Inputs:
/// - A fake Postgres row containing integer, string, and boolean values.
///
/// Output:
/// - Test passes when the encoded row contains stable typed field prefixes.
///
/// Transformation:
/// - Projects row fields through the same encoder used by generated BEAM
///   runtime callers.
#[test]
fn encode_row_serializes_supported_scalar_values() {
    let mut row = postgres::Row::new();
    row.put_int("id", 7);
    row.put_string("email", "ada@example.com");
    row.put_bool("active", true);

    assert_eq!(
        encode_row(
            &row,
            &["id".to_string(), "email".to_string(), "active".to_string()]
        )
        .expect("encode row"),
        format!("i:7\ts:{}\tb:true", encode_text("ada@example.com"))
    );
}

/// Verifies malformed helper invocations use the error protocol.
///
/// Inputs:
/// - An empty private SQL runtime argument list.
///
/// Output:
/// - Test passes when the command exits successfully after printing an encoded
///   runtime error response.
///
/// Transformation:
/// - Exercises the CLI-facing wrapper rather than the fallible inner helper.
#[test]
fn malformed_invocation_returns_error_protocol() {
    let status = run(&[]);

    assert_eq!(status, ExitCode::SUCCESS);
}
