use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::terlan_erlang::emit::quote_erlang_atom_literal;

/// Default maximum runtime for one Erlang tool command.
const DEFAULT_ERLANG_COMMAND_TIMEOUT_SECONDS: u64 = 30;

/// Runs a process while preventing local `erl_crash.dump` files in the workspace.
///
/// Inputs:
/// - `command`: process builder to execute.
/// - `label`: human-readable tool name used in spawn failures.
/// - `erl_crash_dump`: optional path assigned to `ERL_CRASH_DUMP`.
///
/// Output:
/// - `Ok(Output)` when the process starts and exits.
/// - `Err(message)` when the process cannot be spawned or exceeds the
///   configured timeout.
///
/// Transformation:
/// - Adds the Erlang crash-dump environment override, resolves the configured
///   timeout, and delegates to the bounded command runner.
pub(super) fn run_command_with_no_erl_crash_dump(
    command: &mut Command,
    label: &str,
    erl_crash_dump: Option<&Path>,
) -> Result<Output, String> {
    if let Some(path) = erl_crash_dump {
        command.env("ERL_CRASH_DUMP", path);
    }
    run_command_with_timeout(command, label, erlang_command_timeout())
}

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
pub(super) fn run_command_with_timeout(
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
                    "{label} timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(err) => return Err(format!("failed to wait for {label}: {err}")),
        }
    }
}

/// Returns the configured Erlang tool timeout.
///
/// Inputs:
/// - `TERLAN_ERLANG_COMMAND_TIMEOUT_SECONDS`: optional environment override.
///
/// Output:
/// - Positive timeout duration for `erl` and `erlc` subprocesses.
///
/// Transformation:
/// - Parses a positive integer override and falls back to the release-safe
///   default when the variable is absent, invalid, or zero.
fn erlang_command_timeout() -> Duration {
    std::env::var("TERLAN_ERLANG_COMMAND_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_ERLANG_COMMAND_TIMEOUT_SECONDS))
}

/// Quotes text as an Erlang atom literal.
///
/// Inputs:
/// - `atom`: untrusted atom text.
///
/// Output:
/// - Single-quoted Erlang atom literal.
///
/// Transformation:
/// - Delegates escaping and wrapping to the Erlang backend so test runners and
///   emitted Erlang source share one atom-quoting contract.
pub(super) fn quote_erlang_atom(atom: &str) -> String {
    quote_erlang_atom_literal(atom)
}
