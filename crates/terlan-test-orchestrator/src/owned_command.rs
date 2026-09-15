//! Bounded process ownership for build producers which already own their receipts.
use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitCode};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

/// Runs one explicit producer; this mode never builds tools or runs the Rust suite.
pub(super) fn main(arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let (timeout, mut command) = match parse(arguments) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error[build.owner.arguments]: {error}");
            return ExitCode::from(2);
        }
    };
    let shutdown = match crate::shutdown::Shutdown::install() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("error[build.owner.signals]: {error}");
            return ExitCode::FAILURE;
        }
    };
    match ProcessControl::new(timeout)
        .with_cancellation(shutdown.flag())
        .run(&mut command, |_| Ok(()))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error[build.owner.{}]: {}", error.kind, error.detail);
            ExitCode::from(match error.kind {
                "timed-out" => 124,
                "cancelled" => 130,
                _ => 1,
            })
        }
    }
}

fn parse(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<(Duration, Command), &'static str> {
    const USAGE: &str = "expected --timeout-seconds <1..86400> -- <program> [args...]";
    if arguments.next().as_deref() != Some(OsStr::new("--timeout-seconds")) {
        return Err(USAGE);
    }
    let seconds = arguments
        .next()
        .and_then(|value| {
            let value = value.to_str()?;
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            value
                .parse::<u64>()
                .ok()
                .filter(|value| (1..=86400).contains(value))
        })
        .ok_or(USAGE)?;
    if arguments.next().as_deref() != Some(OsStr::new("--")) {
        return Err(USAGE);
    }
    let program = arguments
        .next()
        .filter(|value| !value.is_empty())
        .ok_or(USAGE)?;
    let mut command = Command::new(program);
    command.args(arguments);
    Ok((Duration::from_secs(seconds), command))
}
