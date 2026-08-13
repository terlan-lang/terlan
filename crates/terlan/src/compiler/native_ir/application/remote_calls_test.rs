use super::*;

#[test]
fn lowers_primitive_test_assertions_without_std_runtime_calls() {
    let cases = [
        (
            "assert",
            vec![CoreExpr::Var("condition".to_string())],
            CoreExpr::Var("condition".to_string()),
        ),
        (
            "assert_true",
            vec![CoreExpr::Var("condition".to_string())],
            CoreExpr::Var("condition".to_string()),
        ),
        (
            "assert_false",
            vec![CoreExpr::Var("condition".to_string())],
            CoreExpr::UnaryOp {
                operator: "not".to_string(),
                operand: Box::new(CoreExpr::Var("condition".to_string())),
            },
        ),
        ("fail", Vec::new(), CoreExpr::Atom("false".to_string())),
    ];

    for (function, mut args, expected) in cases {
        assert_eq!(test_assertion_expr(function, &mut args), Some(expected));
        assert!(args.is_empty());
    }
}

#[test]
fn lowers_test_comparison_assertions_without_std_runtime_calls() {
    let mut args = vec![CoreExpr::Int(1), CoreExpr::Int(2)];
    assert_eq!(
        test_assertion_expr("assert_not_equal", &mut args),
        Some(CoreExpr::BinaryOp {
            operator: "!=".to_string(),
            left: Box::new(CoreExpr::Int(1)),
            right: Box::new(CoreExpr::Int(2)),
        })
    );
    assert!(args.is_empty());
}

#[test]
fn leaves_unknown_or_malformed_test_calls_for_admission_diagnostics() {
    let mut args = vec![CoreExpr::Atom("true".to_string())];
    assert_eq!(test_assertion_expr("assert_equal", &mut args), None);
    assert_eq!(args, vec![CoreExpr::Atom("true".to_string())]);
}
