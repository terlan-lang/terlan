//! Byte-preserving tool execution using the VM's bounded process owner.

use std::process::{Command, Output, Stdio};
use std::time::Duration;

use super::{capture_command, CaptureFailure};

/// Keeps process failure classification and execution context until presentation.
#[derive(Debug)]
pub(crate) struct ToolCommandError {
    failure: CaptureFailure,
    label: String,
    timeout: Duration,
}

impl ToolCommandError {
    /// Stable process/pipe classification without parsing rendered diagnostics.
    pub(crate) fn code(&self) -> &'static str {
        match &self.failure {
            CaptureFailure::Process(code, _) => code,
            CaptureFailure::MissingProgram(_) => "spawn_failed",
            CaptureFailure::Pipe(error) => error.code(),
        }
    }
}

impl std::fmt::Display for ToolCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = &self.label;
        match &self.failure {
            CaptureFailure::Process("timed_out", _) => write!(
                formatter,
                "{label} timed out after {} milliseconds",
                self.timeout.as_millis()
            ),
            CaptureFailure::Process(_, message) | CaptureFailure::MissingProgram(message) => {
                write!(formatter, "{label} {}: {message}", self.code())
            }
            CaptureFailure::Pipe(error) => {
                write!(formatter, "{label} {}: {}", self.code(), error.message())
            }
        }
    }
}

impl std::error::Error for ToolCommandError {}

/// Captures a noninteractive tool without duplicating process and pipe ownership.
///
/// Inputs:
/// - `command`: exact OS-native arguments, environment, and working directory.
/// - `label`: tool identity used in failures.
/// - `timeout`: deadline covering execution and pipe drainage.
/// - `output_limit`: combined stdout/stderr byte ceiling.
///
/// Output:
/// - Byte-exact output and exit status, or an attributed execution failure.
///
/// Transformation:
/// - Closes stdin, drains both pipes while the child runs, and reuses request
///   cleanup on failure or unwind. Linux/macOS also own the child process group.
pub(crate) fn capture_tool_command(
    command: &mut Command,
    label: &str,
    timeout: Duration,
    output_limit: usize,
) -> Result<Output, ToolCommandError> {
    capture_tool_command_with_launch(command, label, timeout, output_limit, |_| Ok(()))
}

/// Observes a successfully spawned tool while cleanup ownership is still held.
/// Observation failure terminates the child through the same process owner.
pub(crate) fn capture_tool_command_with_launch(
    command: &mut Command,
    label: &str,
    timeout: Duration,
    output_limit: usize,
    launched: impl FnOnce(u32) -> Result<(), String>,
) -> Result<Output, ToolCommandError> {
    capture_tool(command, timeout, output_limit, launched).map_err(|failure| ToolCommandError {
        failure,
        label: label.to_owned(),
        timeout,
    })
}

/// Captures an optional tool; only a missing OS executable may be skipped.
/// Observer, timeout, pipe, permission and nonzero-exit results are not absence.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn capture_optional_tool_command(
    command: &mut Command,
    label: &str,
    timeout: Duration,
    output_limit: usize,
) -> Result<Option<Output>, ToolCommandError> {
    match capture_tool(command, timeout, output_limit, |_| Ok(())) {
        Ok(output) => Ok(Some(output)),
        Err(CaptureFailure::MissingProgram(_)) => Ok(None),
        Err(failure) => Err(ToolCommandError {
            failure,
            label: label.to_owned(),
            timeout,
        }),
    }
}

fn capture_tool(
    command: &mut Command,
    timeout: Duration,
    output_limit: usize,
    launched: impl FnOnce(u32) -> Result<(), String>,
) -> Result<Output, CaptureFailure> {
    if timeout.is_zero() || output_limit == 0 {
        return Err(CaptureFailure::Process(
            "invalid_limits",
            "requires positive timeout and output limits".into(),
        ));
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    capture_command(command, Vec::new(), timeout, output_limit, None, launched)
}
