//! Byte-preserving tool execution using the VM's bounded process owner.

use std::process::{Command, Output, Stdio};
use std::time::Duration;

use super::{capture_command, CaptureFailure};

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
) -> Result<Output, String> {
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
) -> Result<Output, String> {
    capture_tool(command, timeout, output_limit, launched)
        .map_err(|failure| describe_failure(failure, label, timeout))
}

/// Captures an optional tool; only a missing OS executable may be skipped.
/// Observer, timeout, pipe, permission and nonzero-exit results are not absence.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn capture_optional_tool_command(
    command: &mut Command,
    label: &str,
    timeout: Duration,
    output_limit: usize,
) -> Result<Option<Output>, String> {
    match capture_tool(command, timeout, output_limit, |_| Ok(())) {
        Ok(output) => Ok(Some(output)),
        Err(CaptureFailure::MissingProgram(_)) => Ok(None),
        Err(failure) => Err(describe_failure(failure, label, timeout)),
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

fn describe_failure(failure: CaptureFailure, label: &str, timeout: Duration) -> String {
    match failure {
        CaptureFailure::Process("timed_out", _) => {
            format!(
                "{label} timed out after {} milliseconds",
                timeout.as_millis()
            )
        }
        CaptureFailure::Process(code, message) => format!("{label} {code}: {message}"),
        CaptureFailure::MissingProgram(message) => format!("{label} spawn_failed: {message}"),
        CaptureFailure::Pipe(error) => {
            format!("{label} {}: {}", error.code(), error.message())
        }
    }
}
