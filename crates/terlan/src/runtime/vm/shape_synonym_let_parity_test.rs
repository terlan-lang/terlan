use super::{evaluate_vm_source_result, ReplValue};

const GUARDED_LET_SOURCE: &str = r#"
module shape_synonym_let_parity.

shape Positive(value) =
    value where value > 0.

pub run(): Int ->
    let candidate = INPUT;
    let Positive(value) = candidate;
    value.
"#;

#[test]
fn guarded_shape_let_binds_when_its_guard_passes() {
    let value = evaluate_vm_source_result(
        "<guarded-shape-let-success>.terl",
        &GUARDED_LET_SOURCE.replace("INPUT", "7"),
    )
    .expect("guarded shape let assertion should match");

    assert_eq!(value, ReplValue::Int(7));
}

#[test]
fn guarded_shape_let_fails_without_committing_bindings() {
    let error = evaluate_vm_source_result(
        "<guarded-shape-let-failure>.terl",
        &GUARDED_LET_SOURCE.replace("INPUT", "-1"),
    )
    .expect_err("guarded shape let assertion should fail");

    assert_eq!(error, "no case clause matched -1");
}
