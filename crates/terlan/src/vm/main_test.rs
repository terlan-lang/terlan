#[cfg(test)]
#[path = "main_test/native_transition_test.rs"]
#[cfg(test)]
mod native_transition_test;

use super::*;

#[test]
fn retired_http_benchmark_commands_are_unknown() {
    for command in [
        "benchmark-http-handler",
        "benchmark-http-stack",
        "benchmark-http-vm-stream",
        "benchmark-http-socket",
    ] {
        for tail in [vec![], vec!["--help"], vec!["--iterations", "1"]] {
            let args = std::iter::once(command)
                .chain(tail)
                .map(str::to_owned)
                .collect();
            assert!(matches!(
                parse_args(args),
                VmCommand::Error(message)
                    if message == format!("unknown terlan-vm command: {command}")
            ));
        }
    }
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
