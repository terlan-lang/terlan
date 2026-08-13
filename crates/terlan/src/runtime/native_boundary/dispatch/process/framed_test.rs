use super::{run_process_length_framed, valid_header_name};
use crate::runtime::native_boundary::dispatch::NativeBoundaryValue;

#[test]
fn header_name_accepts_tokens_and_rejects_protocol_injection() {
    assert!(valid_header_name("Content-Length"));
    assert!(valid_header_name("X_Frame.Length"));
    assert!(!valid_header_name(""));
    assert!(!valid_header_name("Content-Length: 1"));
    assert!(!valid_header_name("Content Length"));
    assert!(!valid_header_name("Content-Length\r\nInjected"));
}

#[test]
fn phased_session_flushes_each_input_before_admitting_the_next() {
    let script = concat!(
        "IFS= read -r first; ",
        "printf 'Content-Length: %s\\r\\n\\r\\n%s' \"${#first}\" \"$first\"; ",
        "IFS= read -r second; ",
        "printf 'Content-Length: %s\\r\\n\\r\\n%s' \"${#second}\" \"$second\"; ",
        "exit 7"
    );
    let value = run_process_length_framed(
        &[framed_request(
            script,
            vec![("first\n", 1), ("second\n", 1)],
        )],
        None,
    )
    .expect("framed dispatch");
    let NativeBoundaryValue::Record { name, fields } = value else {
        panic!("expected Result record")
    };
    assert_eq!(name, "Ok");
    let Some((_, NativeBoundaryValue::Record { fields, .. })) = fields.first() else {
        panic!("expected FramedOutput")
    };
    assert_eq!(record_int(fields, "status"), Some(7));
    assert_eq!(
        record_text_list(fields, "frames"),
        Some(vec!["first".to_string(), "second".to_string()])
    );
}

#[test]
fn framed_session_rejects_a_missing_length_header() {
    let value = run_process_length_framed(
        &[framed_request(
            "printf 'Wrong-Length: 2\\r\\n\\r\\nok'",
            vec![("request\n", 1)],
        )],
        None,
    )
    .expect("framed dispatch");
    let NativeBoundaryValue::Record { name, fields } = value else {
        panic!("expected Result record")
    };
    assert_eq!(name, "Err");
    let Some((_, NativeBoundaryValue::Record { fields, .. })) = fields.first() else {
        panic!("expected ProcessError")
    };
    assert_eq!(record_atom(fields, "code"), Some("invalid_frame"));
}

fn framed_request(script: &str, exchanges: Vec<(&str, i64)>) -> NativeBoundaryValue {
    NativeBoundaryValue::Record {
        name: "FramedRequest".to_string(),
        fields: vec![
            ("command".to_string(), command(script)),
            (
                "exchanges".to_string(),
                NativeBoundaryValue::List(
                    exchanges
                        .into_iter()
                        .map(|(input, expected_frames)| NativeBoundaryValue::Record {
                            name: "FramedExchange".to_string(),
                            fields: vec![
                                (
                                    "input".to_string(),
                                    NativeBoundaryValue::Text(input.to_string()),
                                ),
                                (
                                    "expected_frames".to_string(),
                                    NativeBoundaryValue::Int(expected_frames),
                                ),
                            ],
                        })
                        .collect(),
                ),
            ),
            (
                "length_header".to_string(),
                NativeBoundaryValue::Text("Content-Length".to_string()),
            ),
        ],
    }
}

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
                NativeBoundaryValue::List(vec![
                    NativeBoundaryValue::Text("-c".to_string()),
                    NativeBoundaryValue::Text(script.to_string()),
                ]),
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
            ("timeout_ms".to_string(), NativeBoundaryValue::Int(2_000)),
            (
                "output_limit_bytes".to_string(),
                NativeBoundaryValue::Int(1_048_576),
            ),
        ],
    }
}

fn record_int(fields: &[(String, NativeBoundaryValue)], name: &str) -> Option<i64> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then(|| match value {
                NativeBoundaryValue::Int(value) => Some(*value),
                _ => None,
            })
            .flatten()
    })
}

fn record_atom<'a>(fields: &'a [(String, NativeBoundaryValue)], name: &str) -> Option<&'a str> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then(|| match value {
                NativeBoundaryValue::Atom(value) => Some(value.as_str()),
                _ => None,
            })
            .flatten()
    })
}

fn record_text_list(fields: &[(String, NativeBoundaryValue)], name: &str) -> Option<Vec<String>> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then(|| match value {
                NativeBoundaryValue::List(values) => values
                    .iter()
                    .map(|value| match value {
                        NativeBoundaryValue::Text(value) => Some(value.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => None,
            })
            .flatten()
    })
}
