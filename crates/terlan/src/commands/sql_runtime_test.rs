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

/// Verifies query parameter decoding accepts JSON arrays only.
///
/// Inputs:
/// - Base64-encoded JSON array.
/// - Base64-encoded JSON object.
///
/// Output:
/// - Array input decodes into NativeBoundary JSON values.
/// - Object input returns a stable protocol error.
///
/// Transformation:
/// - Validates the generated helper parameter contract before any database
///   connection is attempted.
#[test]
fn decode_params_accepts_arrays_and_rejects_non_arrays() {
    let params = decode_params(&encode_text("[1, true, \"Ada\"]")).expect("decode params");
    assert_eq!(params.len(), 3);

    assert_eq!(
        decode_params(&encode_text("{\"id\":1}")).expect_err("object params should fail"),
        "SQL runtime params must be a JSON array"
    );
}

/// Verifies malformed JSON parameters produce stable protocol diagnostics.
///
/// Inputs:
/// - Base64-encoded invalid JSON text.
///
/// Output:
/// - Error text identifies the SQL runtime params channel.
///
/// Transformation:
/// - Keeps malformed helper payloads out of the Postgres adapter.
#[test]
fn decode_params_rejects_malformed_json() {
    let error = decode_params(&encode_text("[1,")).expect_err("malformed params should fail");

    assert!(
        error.contains("SQL runtime params are not valid JSON"),
        "{error}"
    );
}

/// Verifies driver-decoded scalar values encode into the SQL runtime protocol.
#[test]
fn encode_decoded_value_serializes_supported_scalar_values() {
    assert_eq!(encode_decoded_value(VmPostgresDecodedValue::Int(7)), "i:7");
    assert_eq!(
        encode_decoded_value(VmPostgresDecodedValue::String(
            "ada@example.com".to_string()
        )),
        format!("s:{}", encode_text("ada@example.com"))
    );
    assert_eq!(
        encode_decoded_value(VmPostgresDecodedValue::Bool(true)),
        "b:true"
    );
}

/// Verifies JSON and null values retain distinct protocol representations.
#[test]
fn encode_decoded_value_serializes_json_and_null_values() {
    assert_eq!(
        encode_decoded_value(VmPostgresDecodedValue::Json(
            "{\"role\":\"admin\"}".to_string()
        )),
        format!("j:{}", encode_text("{\"role\":\"admin\"}"))
    );
    assert_eq!(encode_decoded_value(VmPostgresDecodedValue::Null), "n:");
}

/// Verifies transaction-only SQL cannot fall through to autocommit dispatch.
#[test]
fn transaction_requirements_reject_unsafe_helper_dispatch() {
    assert_eq!(
        SqlRuntimeTransactionRequirement::ActiveTransactionRequired
            .require_autocommit()
            .expect_err("transaction-only SQL must fail"),
        "SQL operation requires an active typed VM transaction; autocommit dispatch is forbidden"
    );
    assert_eq!(
        SqlRuntimeTransactionRequirement::VmManagedControl
            .require_autocommit()
            .expect_err("VM-managed control must fail"),
        "SQL transaction control is VM-owned and cannot execute through the SQL runtime helper"
    );
}

/// Verifies transaction requirements fail before database configuration lookup.
#[test]
fn run_inner_rejects_transaction_only_sql_before_database_lookup() {
    let args = vec![
        "query".to_string(),
        "active_transaction_required".to_string(),
        encode_text("SELECT id FROM users FOR UPDATE"),
        encode_text("[]"),
        encode_text("id"),
    ];

    assert_eq!(
        run_inner(&args).expect_err("transaction context is required"),
        "SQL operation requires an active typed VM transaction; autocommit dispatch is forbidden"
    );
}

/// Verifies unsupported operations fail before database configuration lookup.
///
/// Inputs:
/// - Private SQL helper arguments with an unsupported operation and otherwise
///   valid encoded payloads.
///
/// Output:
/// - Error reports the unsupported operation, independent of database
///   environment variables.
///
/// Transformation:
/// - Ensures malformed generated SQL helper calls never open a Postgres pool.
#[test]
fn run_inner_rejects_unsupported_operation_before_database_lookup() {
    let args = vec![
        "drop_database".to_string(),
        "autocommit_allowed".to_string(),
        encode_text("SELECT 1"),
        encode_text("[]"),
        encode_text(""),
    ];

    assert_eq!(
        run_inner(&args).expect_err("unsupported operation should fail"),
        "unsupported SQL runtime operation `drop_database`"
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
