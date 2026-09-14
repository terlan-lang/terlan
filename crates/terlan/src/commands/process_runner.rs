use std::process::{Command, Output};
use std::time::Duration;

use crate::runtime::native_boundary::dispatch::capture_tool_command;

const TOOL_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

/// Runs a process with stdout/stderr capture and a hard timeout.
///
/// Inputs:
/// - `command`: process builder to spawn.
/// - `label`: human-readable tool name used in diagnostics.
/// - `timeout`: maximum duration to wait for process completion.
///
/// Output:
/// - `Ok(Output)` when the child exits before the timeout.
/// - `Err(message)` when spawning fails, waiting fails, or the child times out.
///
/// Transformation:
/// - Closes stdin and concurrently drains byte-bounded output through the shared
///   process owner. Execution and pipe drainage share the same deadline.
pub(crate) fn run_command_with_timeout(
    command: &mut Command,
    label: &str,
    timeout: Duration,
) -> Result<Output, String> {
    capture_tool_command(command, label, timeout, TOOL_OUTPUT_LIMIT)
        .map_err(|error| error.to_string())
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "process_runner_test.rs"]
mod tests;
