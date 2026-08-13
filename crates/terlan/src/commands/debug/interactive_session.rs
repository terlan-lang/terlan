//! Command-at-a-time break loop for one stopped native VM shard.

use crate::runtime::native_image::debug::TvmNativeDebugRecord;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;

use super::execution::{NativeDebuggerExecutionReport, NativeDebuggerRuntime};
use super::presentation::{emit_interactive_error, emit_interactive_event};
use super::session::DebugBreakpointResolution;
use super::DebugCliError;

pub(super) fn execute_interactive_debug_session(
    shard: &mut PureNativeExecutionShard,
    source_records: &[TvmNativeDebugRecord],
    initial_breakpoints: &[DebugBreakpointResolution],
    json_events: bool,
    entry_hint: Option<&str>,
) -> Result<NativeDebuggerExecutionReport, DebugCliError> {
    let mut completions = source_records
        .iter()
        .flat_map(|record| {
            [
                record.module.clone(),
                format!("{}.{}", record.module, record.function),
            ]
        })
        .collect::<Vec<_>>();
    completions.extend(
        source_records
            .iter()
            .flat_map(|record| record.continuation_spans.iter())
            .flat_map(|continuation| continuation.local_names.iter())
            .filter(|name| !name.starts_with('$'))
            .cloned(),
    );
    completions.extend((0..64).map(|index| format!("${index}")));
    completions.sort();
    completions.dedup();
    let mut reader = super::input::DebugCommandReader::open(completions)?;
    let mut runtime =
        NativeDebuggerRuntime::new(shard, source_records, initial_breakpoints, entry_hint);
    loop {
        let command = match reader.next_command() {
            Ok(Some(command)) => command,
            Ok(None) => break,
            Err(error) if error.code != "debug_input_failed" => {
                emit_interactive_error(&error, json_events);
                continue;
            }
            Err(error) => return Err(error),
        };
        let event_start = runtime.event_count();
        if let Err(message) = runtime.execute(&command) {
            emit_interactive_error(
                &DebugCliError {
                    code: "debug_command_failed",
                    message: format!("line {}: {message}", command.line),
                },
                json_events,
            );
            continue;
        }
        for event in runtime.events_from(event_start) {
            emit_interactive_event(event, json_events);
        }
        if command.name == "quit" {
            break;
        }
    }
    runtime.finish()
}
