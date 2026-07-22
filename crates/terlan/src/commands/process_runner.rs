use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
/// - Spawns the command with captured output, polls for completion, kills the
///   child on timeout, and preserves normal `Command::output`-style results
///   for successful waits.
pub(crate) fn run_command_with_timeout(
    command: &mut Command,
    label: &str,
    timeout: Duration,
) -> Result<Output, String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to run {label}: {err}"))?;
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("failed to collect {label} output: {err}"));
            }
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait_with_output();
                return Err(format!(
                    "{label} timed out after {} milliseconds",
                    timeout.as_millis()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(err) => return Err(format!("failed to wait for {label}: {err}")),
        }
    }
}
