use super::*;

#[test]
fn test_eval_accepts_true_bool() {
    assert_eq!(evaluate_test_result(ReplValue::Bool(true)), Ok(()));
}

#[test]
fn test_eval_rejects_false_bool() {
    let error = evaluate_test_result(ReplValue::Bool(false)).expect_err("false should fail");
    assert!(error.contains("returned false"));
}

#[test]
fn test_eval_rejects_non_bool_return() {
    let error = evaluate_test_result(ReplValue::Int(1)).expect_err("int should fail");
    assert!(error.contains("expects Bool return"));
}
