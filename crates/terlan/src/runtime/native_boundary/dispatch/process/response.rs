//! Typed process completion and error values shared by execution modes.

use super::NativeBoundaryValue;

/// Encodes a completed command without changing its exit status or captured streams.
pub(super) fn process_output(status: i64, stdout: String, stderr: String) -> NativeBoundaryValue {
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "Output".to_string(),
            fields: vec![
                ("status".to_string(), NativeBoundaryValue::Int(status)),
                ("stdout".to_string(), NativeBoundaryValue::Text(stdout)),
                ("stderr".to_string(), NativeBoundaryValue::Text(stderr)),
            ],
        },
    )
}

/// Encodes the ordered responses of a framed command.
pub(super) fn framed_process_output(
    status: i64,
    frames: Vec<String>,
    stderr: String,
) -> NativeBoundaryValue {
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "FramedOutput".to_string(),
            fields: vec![
                ("status".to_string(), NativeBoundaryValue::Int(status)),
                (
                    "frames".to_string(),
                    NativeBoundaryValue::List(
                        frames.into_iter().map(NativeBoundaryValue::Text).collect(),
                    ),
                ),
                ("stderr".to_string(), NativeBoundaryValue::Text(stderr)),
            ],
        },
    )
}

/// Preserves the stable process error code, message, and program identity.
pub(super) fn process_error(
    code: &str,
    message: impl Into<String>,
    program: impl Into<String>,
) -> NativeBoundaryValue {
    result_record(
        "Err",
        "reason",
        NativeBoundaryValue::Record {
            name: "ProcessError".to_string(),
            fields: vec![
                (
                    "code".to_string(),
                    NativeBoundaryValue::Atom(code.to_string()),
                ),
                (
                    "message".to_string(),
                    NativeBoundaryValue::Text(message.into()),
                ),
                (
                    "program".to_string(),
                    NativeBoundaryValue::Text(program.into()),
                ),
            ],
        },
    )
}

/// Encodes batch completions in their original command order.
pub(super) fn process_batch_output(values: Vec<NativeBoundaryValue>) -> NativeBoundaryValue {
    let completions = values.into_iter().map(batch_completion).collect::<Vec<_>>();
    result_record(
        "Ok",
        "value",
        NativeBoundaryValue::Record {
            name: "BatchOutput".to_string(),
            fields: vec![(
                "completions".to_string(),
                NativeBoundaryValue::List(completions),
            )],
        },
    )
}

fn batch_completion(value: NativeBoundaryValue) -> NativeBoundaryValue {
    let (output, error) = match value {
        NativeBoundaryValue::Record { name, mut fields } if name == "Ok" => {
            let value = fields
                .pop()
                .map(|(_, value)| value)
                .unwrap_or(NativeBoundaryValue::Unit);
            (some_record(value), none_record())
        }
        NativeBoundaryValue::Record { name, mut fields } if name == "Err" => {
            let value = fields
                .pop()
                .map(|(_, value)| value)
                .unwrap_or(NativeBoundaryValue::Unit);
            (none_record(), some_record(value))
        }
        _ => (none_record(), none_record()),
    };
    NativeBoundaryValue::Record {
        name: "BatchCompletion".to_string(),
        fields: vec![("output".to_string(), output), ("error".to_string(), error)],
    }
}

fn some_record(value: NativeBoundaryValue) -> NativeBoundaryValue {
    result_record("Some", "value", value)
}

fn none_record() -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "None".to_string(),
        fields: Vec::new(),
    }
}

fn result_record(name: &str, field: &str, value: NativeBoundaryValue) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: name.to_string(),
        fields: vec![(field.to_string(), value)],
    }
}
