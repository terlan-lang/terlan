#[cfg(test)]
#[path = "main_test/native_transition_test.rs"]
#[cfg(test)]
mod native_transition_test;

use super::*;
#[path = "main_test/storage_lifecycle_test.rs"]
mod storage_lifecycle_test;

#[test]
fn run_arguments_storage_authority_is_explicit_and_separate_from_script_arguments() {
    let args = [
        "app.tvm",
        "--storage",
        "primary@0101010101010101010101010101010101010101010101010101010101010101=/srv/checkpoints",
        "--",
        "--storage",
        "untrusted=/tmp/untrusted",
    ]
    .map(str::to_string);
    let VmCommand::Run {
        storage_bindings,
        program_arguments,
        ..
    } = parse_run_args(&args)
    else {
        panic!("valid binding rejected")
    };
    assert_eq!(
        storage_bindings,
        vec![VmStorageBinding::parse("primary@0101010101010101010101010101010101010101010101010101010101010101=/srv/checkpoints").unwrap()]
    );
    assert_eq!(program_arguments, ["--storage", "untrusted=/tmp/untrusted"]);
    for invalid in [
        "primary=relative",
        "= /tmp",
        "Primary=/tmp",
        "../primary=/tmp",
        "primary",
        "primary=",
    ] {
        assert!(matches!(
            parse_run_args(&["app.tvm".into(), "--storage".into(), invalid.into()]),
            VmCommand::Error(_)
        ));
    }
    assert!(matches!(
        parse_run_args(&["app.tvm".into(), "--storage".into()]),
        VmCommand::Error(_)
    ));
}

#[test]
fn run_arguments_select_script_result_propagation() {
    let command = parse_run_args(&[
        "application.tvm".to_string(),
        "--script-eval".to_string(),
        "--".to_string(),
        "input.json".to_string(),
    ]);

    assert!(matches!(
        command,
        VmCommand::Run {
            result_mode: RunResultMode::Script,
            program_arguments,
            ..
        } if program_arguments == ["input.json"]
    ));
}

#[test]
fn run_arguments_reject_conflicting_result_contracts() {
    let command = parse_run_args(&[
        "application.tvm".to_string(),
        "--test-eval".to_string(),
        "--script-eval".to_string(),
    ]);

    assert!(matches!(
        command,
        VmCommand::Error(message)
            if message == "--test-eval and --script-eval are mutually exclusive"
    ));
}

#[test]
fn run_arguments_rejects_script_conflict_via_test_alias() {
    let command = parse_run_args(&[
        "application.tvm".to_string(),
        "--test".to_string(),
        "--script-eval".to_string(),
    ]);

    assert!(matches!(
        command,
        VmCommand::Error(message)
            if message == "--test-eval and --script-eval are mutually exclusive"
    ));
}

#[test]
fn run_arguments_accepts_test_alias() {
    let command = parse_run_args(&["application.tvm".to_string(), "--test".to_string()]);

    assert!(matches!(
        command,
        VmCommand::Run {
            result_mode: RunResultMode::Test,
            ..
        }
    ));
}

#[test]
fn script_result_propagation_is_silent_only_for_unit() {
    assert_eq!(evaluate_script_result(ReplValue::Unit), None);
    assert_eq!(
        evaluate_script_result(ReplValue::Int(42)),
        Some("42".to_string())
    );
    assert_eq!(
        evaluate_script_result(ReplValue::Bool(false)),
        Some("false".to_string())
    );
}
