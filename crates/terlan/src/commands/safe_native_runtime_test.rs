use std::io::Cursor;
use std::process::ExitCode;

use crate::terlan_safenative::runtime::SafeNativeRuntime;
use crate::terlan_safenative::term::{SafeNativeReplyTerm, SafeNativeTerm};

use super::{
    decode_arg, decode_text, encode_ok, encode_reply, encode_text, error_response, execute_line,
    operation_returns_result, run, run_loop,
};

fn error_code(response: &str) -> String {
    let encoded = response
        .split_whitespace()
        .nth(1)
        .expect("encoded error code");
    decode_text(encoded).expect("decoded error code")
}

#[test]
fn run_rejects_private_runtime_arguments() {
    assert_eq!(run(&["unexpected".to_string()]), ExitCode::from(2));
}

#[test]
fn decode_arg_accepts_supported_protocol_terms() {
    assert_eq!(
        decode_arg(&format!("s:{}", encode_text("hello"))),
        Ok(SafeNativeTerm::Text("hello".to_string()))
    );
    assert_eq!(decode_arg("i:-42"), Ok(SafeNativeTerm::Int(-42)));
    assert_eq!(
        decode_arg("h:7:3"),
        Ok(SafeNativeTerm::Handle {
            id: 7,
            generation: 3
        })
    );
    assert_eq!(
        decode_arg(&format!("ls:{},{}", encode_text("a"), encode_text("b"))),
        Ok(SafeNativeTerm::List(vec![
            SafeNativeTerm::Text("a".to_string()),
            SafeNativeTerm::Text("b".to_string())
        ]))
    );
    assert_eq!(decode_arg("ls:"), Ok(SafeNativeTerm::List(Vec::new())));
}

#[test]
fn decode_arg_rejects_bad_protocol_terms_with_stable_codes() {
    assert_eq!(
        error_code(&decode_arg("s:not-base64").expect_err("bad string")),
        "safe_native_bad_string"
    );
    assert_eq!(
        error_code(&decode_arg("i:not-int").expect_err("bad int")),
        "safe_native_bad_int"
    );
    assert_eq!(
        error_code(&decode_arg("h:not-id:1").expect_err("bad handle id")),
        "safe_native_bad_handle"
    );
    assert_eq!(
        error_code(&decode_arg("x:value").expect_err("unsupported arg")),
        "safe_native_unsupported_arg"
    );
}

#[test]
fn encode_reply_preserves_error_fields_as_protocol_safe_text() {
    let response = encode_reply(
        "std.data.json.parse",
        SafeNativeReplyTerm::Error {
            code: "json.parse".to_string(),
            message: "bad json payload".to_string(),
            offset: 4,
        },
    );
    let mut fields = response.split_whitespace();

    assert_eq!(fields.next(), Some("err"));
    assert_eq!(
        fields.next().and_then(decode_text).as_deref(),
        Some("json.parse")
    );
    assert_eq!(
        fields.next().and_then(decode_text).as_deref(),
        Some("bad json payload")
    );
}

#[test]
fn encode_ok_wraps_result_returning_operations() {
    assert!(operation_returns_result("std.data.json.parse"));
    assert!(!operation_returns_result("std.core.Int.to_string"));

    assert_eq!(
        encode_ok(
            "std.data.json.parse",
            SafeNativeTerm::Text("{\"ok\":true}".to_string())
        ),
        format!("result_ok_string {}", encode_text("{\"ok\":true}"))
    );
    assert_eq!(
        encode_ok(
            "std.core.Int.to_string",
            SafeNativeTerm::Text("42".to_string())
        ),
        format!("ok_string {}", encode_text("42"))
    );
}

#[test]
fn execute_line_rejects_malformed_commands_with_stable_codes() {
    let mut runtime = SafeNativeRuntime::new();

    assert_eq!(
        error_code(&execute_line(&mut runtime, "")),
        "safe_native_empty_command"
    );
    assert_eq!(
        error_code(&execute_line(&mut runtime, "ping")),
        "safe_native_unknown_command"
    );
    assert_eq!(
        error_code(&execute_line(&mut runtime, "call")),
        "safe_native_missing_request"
    );
    assert_eq!(
        error_code(&execute_line(&mut runtime, "call 1")),
        "safe_native_missing_operation"
    );
}

#[test]
fn execute_line_dispatches_unknown_operation_as_runtime_error() {
    let mut runtime = SafeNativeRuntime::new();
    let response = execute_line(
        &mut runtime,
        &format!("call 1 {}", encode_text("unknown.op")),
    );

    assert_eq!(error_code(&response), "dispatch.unknown_operation");
}

#[test]
fn run_loop_writes_one_response_per_input_line() {
    let input = Cursor::new(format!("unknown\ncall 1 {}\n", encode_text("unknown.op")));
    let mut output = Vec::new();

    assert_eq!(run_loop(input, &mut output), ExitCode::SUCCESS);

    let text = String::from_utf8(output).expect("utf8 output");
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert_eq!(error_code(lines[0]), "safe_native_unknown_command");
    assert_eq!(error_code(lines[1]), "dispatch.unknown_operation");
}

#[test]
fn error_response_encodes_spaces_in_message() {
    let response = error_response("safe_native_test", "message with spaces");
    let mut fields = response.split_whitespace();

    assert_eq!(fields.next(), Some("err"));
    assert_eq!(
        fields.next().and_then(decode_text).as_deref(),
        Some("safe_native_test")
    );
    assert_eq!(
        fields.next().and_then(decode_text).as_deref(),
        Some("message with spaces")
    );
    assert_eq!(fields.next(), None);
}
