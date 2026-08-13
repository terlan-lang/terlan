use super::{bind_frame_captures, bind_frame_locals, validate_closed_pure_expression};

#[test]
fn pure_eval_rejects_calls_before_execution() {
    let error = validate_closed_pure_expression("Process.spawn(1)")
        .expect_err("effectful call must be rejected");
    assert!(error
        .to_string()
        .starts_with("error[vm.debugger.eval_side_effect]"));
}

#[test]
fn capture_binding_is_token_aware_and_rejects_missing_slots() {
    assert_eq!(
        bind_frame_captures("$0 + 1 == \"$0\"", &["41".to_string()]).expect("capture binding"),
        "(41) + 1 == \"$0\""
    );
    assert!(bind_frame_captures("$1", &["41".to_string()])
        .expect_err("missing capture")
        .to_string()
        .starts_with("error[vm.debugger.local_missing]"));
}

#[test]
fn source_local_binding_is_token_aware() {
    assert_eq!(
        bind_frame_locals(
            "value + value2 == \"value\"",
            &["value".to_string()],
            &["41".to_string()],
        )
        .expect("source local binding"),
        "(41) + value2 == \"value\""
    );
}
