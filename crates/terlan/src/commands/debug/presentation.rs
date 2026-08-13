//! Stable source and value rendering for debugger events.

use crate::runtime::native_image::debug::TvmNativeDebugRecord;
use crate::runtime::vm::debugger_control::VmDebuggerExecutionState;
use crate::runtime::vm::ReplValue;

use super::DebugCliError;

#[cfg(test)]
#[path = "presentation_test.rs"]
mod test;

const MAX_RENDERED_VALUE_CHARS: usize = 4_096;

pub(super) fn render_bounded(value: &str) -> String {
    let mut rendered = value
        .chars()
        .take(MAX_RENDERED_VALUE_CHARS)
        .collect::<String>();
    if value.chars().count() > MAX_RENDERED_VALUE_CHARS {
        rendered.push_str("…<truncated>");
    }
    rendered
}

pub(super) fn render_native_slots(prefix: &str, values: &[i64]) -> String {
    let values = values
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{prefix}${index}=<native-slot:{value}>"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{values}]")
}

pub(super) fn render_capture_values(values: &[ReplValue], names: &[String]) -> String {
    let values = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let selector = names
                .get(index)
                .filter(|name| !name.starts_with('$'))
                .map_or_else(|| format!("${index}"), |name| format!("{name}(${index})"));
            format!("{selector}={}", render_bounded(&value.render()))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{values}]")
}

pub(super) fn state_name(state: VmDebuggerExecutionState) -> &'static str {
    match state {
        VmDebuggerExecutionState::Running => "running",
        VmDebuggerExecutionState::Paused => "paused",
        VmDebuggerExecutionState::Stepping => "stepping",
    }
}

pub(super) fn source_location(record: &TvmNativeDebugRecord, instruction_offset: usize) -> String {
    let source = std::fs::read_to_string(&record.source_file).ok();
    let (line, column) = source
        .as_deref()
        .and_then(|source| {
            source.get(..record.span_start).map(|prefix| {
                let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
                let column = prefix
                    .rsplit_once('\n')
                    .map_or(prefix, |(_, tail)| tail)
                    .chars()
                    .count()
                    + 1;
                (line, column)
            })
        })
        .unwrap_or((0, 0));
    let expression = source
        .as_deref()
        .and_then(|source| source.get(record.span_start..record.span_end))
        .map(str::trim)
        .map(|source| render_bounded(&source.replace('\n', "\\n")))
        .unwrap_or_else(|| "<source unavailable>".to_string());
    format!(
        "{}:{}:{}:{}.{}:{}..{}@vm:{instruction_offset} origin={} expression={expression}",
        record.source_file,
        line,
        column,
        record.module,
        record.function,
        record.span_start,
        record.span_end,
        record.source_origin
    )
}

pub(super) fn emit_interactive_event(event: &str, json_events: bool) {
    if json_events {
        println!(
            "{}",
            serde_json::json!({"command": "debug", "kind": "event", "event": event})
        );
    } else {
        println!("{event}");
    }
}

pub(super) fn emit_interactive_error(error: &DebugCliError, json_events: bool) {
    if json_events {
        println!(
            "{}",
            serde_json::json!({
                "command": "debug",
                "kind": "error",
                "code": error.code,
                "message": error.message
            })
        );
    } else {
        eprintln!("error[{}]: {}", error.code, error.message);
    }
}
