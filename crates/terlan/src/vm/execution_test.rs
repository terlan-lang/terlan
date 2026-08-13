use super::*;

#[test]
fn test_eval_accepts_true_and_rejects_false() {
    assert!(evaluate_test_result(ReplValue::Bool(true)).is_ok());
    assert_eq!(
        evaluate_test_result(ReplValue::Bool(false)),
        Err("terlan-vm test-eval failed: returned false".to_string())
    );
}

#[test]
fn test_eval_rejects_non_bool_return_value() {
    assert_eq!(
        evaluate_test_result(ReplValue::Int(1)),
        Err("terlan-vm test-eval expects Bool return, found 1".to_string())
    );
}
