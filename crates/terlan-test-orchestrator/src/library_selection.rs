//! One bounded native-platform library selection with exact private result records.

use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::{inspect_workspace_log, ExpectedTests};
use crate::test_result_log::TestResultLog;
use crate::{process_failure, PhaseFailure};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

/// Runs a nonempty exact selection, never falling through to the canonical suite.
pub(super) fn main(arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let (target, expected) = match parse(arguments) {
        Ok(parsed) => parsed,
        Err(detail) => {
            eprintln!("error[rust.selection.arguments]: {detail}");
            return ExitCode::from(2);
        }
    };
    match run(&target, &expected) {
        Ok(()) => {
            println!(
                "[rust-selection] verified {} exact library tests",
                expected.passed.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[rust.selection.{}]: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

fn parse(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<(PathBuf, ExpectedTests), String> {
    const USAGE: &str = "expected --target-dir <directory> -- <exact-test-name>...";
    if arguments.next().as_deref() != Some(OsStr::new("--target-dir")) {
        return Err(USAGE.into());
    }
    let target = arguments
        .next()
        .filter(|value| !value.is_empty())
        .ok_or(USAGE)?;
    if arguments.next().as_deref() != Some(OsStr::new("--")) {
        return Err(USAGE.into());
    }
    let names = arguments
        .map(|value| value.into_string().map_err(|_| USAGE))
        .collect::<Result<Vec<_>, _>>()?;
    if names.is_empty()
        || names.iter().any(|name| {
            !name.contains("::")
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_:".contains(&byte))
        })
    {
        return Err(USAGE.into());
    }
    let expected =
        ExpectedTests::native_names(&serde_json::json!(names)).map_err(|error| error.detail)?;
    Ok((PathBuf::from(target), expected))
}

fn run(target: &Path, expected: &ExpectedTests) -> Result<(), PhaseFailure> {
    let environment = ExecutionEnvironment::capture()?;
    let shutdown = crate::shutdown::Shutdown::install().map_err(|detail| PhaseFailure {
        outcome: "signals",
        detail: detail.to_string(),
    })?;
    let control = ProcessControl::new(Duration::from_secs(1800)).with_cancellation(shutdown.flag());
    let started = Instant::now();
    let log = TestResultLog::create(&environment)?;
    let mut command = environment.test_command(Path::new("cargo"));
    command
        .args(["test", "--locked", "--target-dir"])
        .arg(target)
        .args([
            "--no-default-features",
            "--features",
            "native-codegen",
            "-p",
            "terlan",
            "--lib",
            "--",
            "--exact",
            "--test-threads=1",
            "--color=never",
            "--logfile",
        ])
        .arg(log.path())
        .args(&expected.passed);
    control
        .run(&mut command, |_| Ok(()))
        .map_err(process_failure)?;
    control.check(started).map_err(process_failure)?;
    let (bytes, identity) = log.read(control)?;
    // Reuse the canonical private-channel parser: zero tests, missing names,
    // ignored names, duplicates and forged child stdout cannot satisfy this.
    inspect_workspace_log(&bytes, expected, &identity)?;
    log.close()
}
