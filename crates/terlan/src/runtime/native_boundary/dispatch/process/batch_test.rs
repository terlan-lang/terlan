//! Behavioral coverage for bounded and fail-fast process batches.

use super::*;

fn command(script: &str) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "Command".to_string(),
        fields: vec![
            (
                "program".to_string(),
                NativeBoundaryValue::Text("/bin/sh".to_string()),
            ),
            (
                "arguments".to_string(),
                NativeBoundaryValue::List(
                    ["-c", script]
                        .into_iter()
                        .map(|value| NativeBoundaryValue::Text(value.to_string()))
                        .collect(),
                ),
            ),
            (
                "working_directory".to_string(),
                NativeBoundaryValue::OptionalText(None),
            ),
            ("environment".to_string(), NativeBoundaryValue::List(vec![])),
            (
                "removed_environment".to_string(),
                NativeBoundaryValue::List(vec![]),
            ),
            (
                "stdin".to_string(),
                NativeBoundaryValue::Text(String::new()),
            ),
            ("timeout_ms".to_string(), NativeBoundaryValue::Int(4_000)),
            (
                "output_limit_bytes".to_string(),
                NativeBoundaryValue::Int(4_096),
            ),
        ],
    }
}

fn batch(
    commands: Vec<NativeBoundaryValue>,
    fail_fast: NativeBoundaryValue,
) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "BatchRequest".to_string(),
        fields: vec![
            ("commands".to_string(), NativeBoundaryValue::List(commands)),
            ("max_concurrency".to_string(), NativeBoundaryValue::Int(2)),
            ("fail_fast".to_string(), fail_fast),
        ],
    }
}

fn legacy_batch(commands: Vec<NativeBoundaryValue>) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "BatchRequest".to_string(),
        fields: vec![
            ("commands".to_string(), NativeBoundaryValue::List(commands)),
            ("max_concurrency".to_string(), NativeBoundaryValue::Int(2)),
        ],
    }
}

fn result_name(value: &NativeBoundaryValue) -> &str {
    let NativeBoundaryValue::Record { name, .. } = value else {
        panic!("expected result record")
    };
    name
}

fn batch_completion_names(value: &NativeBoundaryValue) -> Vec<(&str, &str)> {
    let NativeBoundaryValue::Record { fields, .. } = value else {
        panic!("expected outer result")
    };
    let NativeBoundaryValue::Record { fields, .. } = field(fields, "value").unwrap() else {
        panic!("expected batch output")
    };
    let NativeBoundaryValue::List(completions) = field(fields, "completions").unwrap() else {
        panic!("expected completions")
    };
    completions
        .iter()
        .map(|completion| {
            let NativeBoundaryValue::Record { fields, .. } = completion else {
                panic!("expected completion")
            };
            (
                result_name(field(fields, "output").unwrap()),
                result_name(field(fields, "error").unwrap()),
            )
        })
        .collect()
}

#[test]
fn fail_fast_rejects_a_non_boolean_policy() {
    let result = run_process_many(
        &[batch(vec![], NativeBoundaryValue::Text("yes".to_string()))],
        None,
    )
    .unwrap();

    assert_eq!(result_name(&result), "Err");
}

#[test]
fn fail_fast_cancels_a_running_sibling_and_preserves_order() {
    let started = Instant::now();
    let result = run_process_many(
        &[batch(
            vec![command("exit 7"), command("sleep 3")],
            NativeBoundaryValue::Bool(true),
        )],
        None,
    )
    .unwrap();

    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(result_name(&result), "Ok");
    assert_eq!(
        batch_completion_names(&result),
        vec![("Some", "None"), ("None", "Some")]
    );
}

#[test]
fn ordinary_batch_keeps_running_after_a_nonzero_status() {
    let result = run_process_many(
        &[batch(
            vec![command("exit 7"), command("printf complete")],
            NativeBoundaryValue::Bool(false),
        )],
        None,
    )
    .unwrap();

    assert_eq!(
        batch_completion_names(&result),
        vec![("Some", "None"), ("Some", "None")]
    );
}

#[test]
fn legacy_batch_shape_defaults_to_non_fail_fast_execution() {
    let result = run_process_many(
        &[legacy_batch(vec![
            command("exit 7"),
            command("printf complete"),
        ])],
        None,
    )
    .unwrap();

    assert_eq!(
        batch_completion_names(&result),
        vec![("Some", "None"), ("Some", "None")]
    );
}

#[test]
fn process_success_requires_a_zero_status_output_result() {
    assert!(process_result_succeeded(&process_output(
        0,
        String::new(),
        String::new()
    )));
    assert!(!process_result_succeeded(&process_output(
        1,
        String::new(),
        String::new()
    )));
    assert!(!process_result_succeeded(&NativeBoundaryValue::Unit));
    assert!(!process_result_succeeded(&NativeBoundaryValue::Record {
        name: "Ok".to_string(),
        fields: vec![("value".to_string(), NativeBoundaryValue::Unit)],
    }));
}
