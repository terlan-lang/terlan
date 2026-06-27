use serde_json::{json, Map, Value};

use crate::DiagnosticFormat;

/// Emits a REPL event in text or JSON mode.
///
/// Inputs:
/// - `diagnostic_format`: output mode selected by global flags.
/// - `kind`: event kind string.
/// - `fields`: optional pre-rendered JSON fields for JSON mode.
/// - `text`: human-readable text payload.
///
/// Output:
/// - No return value; writes to stdout or stderr.
///
/// Transformation:
/// - Converts REPL events into stable JSON records or text-mode messages.
pub(super) fn emit_repl_event(
    diagnostic_format: DiagnosticFormat,
    kind: &str,
    fields: &[ReplJsonField],
    text: &str,
) {
    match diagnostic_format {
        DiagnosticFormat::Text { .. } => {
            if kind == "error" && text.is_empty() {
                eprintln!("{kind}");
            } else {
                println!("{text}");
            }
        }
        DiagnosticFormat::Json => {
            println!("{}", render_repl_json_event(kind, fields, text));
        }
    }
}

/// Stores one optional JSON property for a structured REPL event.
///
/// Inputs:
/// - `name`: stable event-field name.
/// - `value`: JSON value to attach to the event.
///
/// Output:
/// - Field record consumed by `render_repl_json_event`.
///
/// Transformation:
/// - Keeps event field names and values paired so call sites do not assemble
///   JSON object fragments by hand.
#[derive(Clone, Debug)]
pub(super) struct ReplJsonField {
    name: &'static str,
    value: Value,
}

/// Creates one structured REPL JSON field.
///
/// Inputs:
/// - `name`: stable field name for the REPL event schema.
/// - `value`: value convertible into `serde_json::Value`.
///
/// Output:
/// - A `ReplJsonField` containing the property name and serialized value.
///
/// Transformation:
/// - Converts Rust strings, arrays, and scalar values into serde JSON values
///   before rendering the event object.
pub(super) fn repl_json_field(name: &'static str, value: impl Into<Value>) -> ReplJsonField {
    ReplJsonField {
        name,
        value: value.into(),
    }
}

/// Renders one structured REPL event as a JSON object.
///
/// Inputs:
/// - `kind`: stable REPL event kind.
/// - `fields`: optional structured JSON fields to include in the event.
/// - `text`: human-readable event text.
///
/// Output:
/// - A single-line JSON object with schema, kind, optional fields, and text.
///
/// Transformation:
/// - Builds the event through `serde_json` so escaping, array serialization,
///   and object formatting are delegated to the JSON library.
pub(super) fn render_repl_json_event(kind: &str, fields: &[ReplJsonField], text: &str) -> String {
    let mut event = Map::new();
    event.insert("schema".to_string(), json!("terlan-repl-event-v1"));
    event.insert("kind".to_string(), json!(kind));
    for field in fields {
        event.insert(field.name.to_string(), field.value.clone());
    }
    event.insert("text".to_string(), json!(text));
    serde_json::to_string(&Value::Object(event)).expect("serialize REPL JSON event")
}

/// Emits a successful REPL expression result.
///
/// Inputs:
/// - `diagnostic_format`: output mode selected by global flags.
/// - `value`: rendered Terlan value from expression execution.
///
/// Output:
/// - No return value; writes a result event.
///
/// Transformation:
/// - Normalizes empty output to `Unit` and otherwise includes the rendered
///   value.
pub(super) fn emit_repl_result(diagnostic_format: DiagnosticFormat, value: &str) {
    if value.trim().is_empty() {
        emit_repl_event(
            diagnostic_format,
            "result",
            &[repl_json_field("value", "Unit")],
            "Unit",
        );
    } else {
        emit_repl_event(
            diagnostic_format,
            "result",
            &[repl_json_field("value", value)],
            value,
        );
    }
}
