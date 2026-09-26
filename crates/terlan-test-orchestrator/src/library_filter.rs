//! Compile once, discover a nonempty selection, then verify every private result.

use crate::cargo_artifact_stream::CargoArtifactStream;
use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::{inspect_workspace_log, ExpectedTests};
use crate::test_result_log::TestResultLog;
use crate::{process_failure, PhaseFailure};
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

struct Selection {
    target_directory: PathBuf,
    filter: String,
    sanitizer: bool,
}

/// Runs only an admitted nonempty library filter, never the canonical suite.
pub(super) fn main(arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let selection = match parse(arguments) {
        Ok(selection) => selection,
        Err(detail) => {
            eprintln!("error[rust.filter.arguments]: {detail}");
            return ExitCode::from(2);
        }
    };
    match run(selection) {
        Ok(count) => {
            println!("[rust-filter] verified {count} runnable library tests");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[rust.filter.{}]: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

fn parse(mut arguments: impl Iterator<Item = OsString>) -> Result<Selection, &'static str> {
    const USAGE: &str =
        "expected --target-dir <directory> --filter <namespace> [--thread-sanitizer]";
    if arguments.next().as_deref() != Some(OsStr::new("--target-dir")) {
        return Err(USAGE);
    }
    let directory = arguments
        .next()
        .filter(|value| !value.is_empty())
        .ok_or(USAGE)?;
    if arguments.next().as_deref() != Some(OsStr::new("--filter")) {
        return Err(USAGE);
    }
    let filter = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(USAGE)?;
    if filter.is_empty()
        || !filter
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_:".contains(&byte))
    {
        return Err(USAGE);
    }
    let sanitizer = match arguments.next().as_deref() {
        None => false,
        Some(value) if value == "--thread-sanitizer" => true,
        _ => return Err(USAGE),
    };
    if arguments.next().is_some() {
        return Err(USAGE);
    }
    Ok(Selection {
        target_directory: directory.into(),
        filter,
        sanitizer,
    })
}

fn run(selection: Selection) -> Result<usize, PhaseFailure> {
    let environment = ExecutionEnvironment::capture()?;
    let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
    let control = ProcessControl::new(Duration::from_secs(1800)).with_cancellation(shutdown.flag());
    let root = home::env::Env::current_dir(&environment).map_err(failure)?;
    let cargo = resolve_program("cargo", &environment.value("PATH").unwrap_or_default())?;
    let mut executables = ExecutableBinding::capture(&[("cargo", cargo.clone())], control)?;
    let mut command = environment.test_command(&cargo);
    if selection.sanitizer {
        if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            return Err(failure("ThreadSanitizer selection requires Linux x86-64"));
        }
        let flags = environment
            .value("RUSTFLAGS")
            .and_then(|value| value.into_string().ok())
            .unwrap_or_default();
        if !flags
            .split_whitespace()
            .any(|flag| flag == "-Zsanitizer=thread")
            || flags
                .split_whitespace()
                .any(|flag| flag.starts_with("-Zsanitizer=") && flag != "-Zsanitizer=thread")
            || environment.value("CARGO_ENCODED_RUSTFLAGS").is_some()
            || environment.value("TSAN_OPTIONS").as_deref() != Some(OsStr::new("halt_on_error=1"))
        {
            return Err(failure(
                "ThreadSanitizer requires explicit thread instrumentation and fail-fast options",
            ));
        }
        command.args([
            "+nightly-2026-07-16",
            "test",
            "-Zbuild-std",
            "--target",
            "x86_64-unknown-linux-gnu",
        ]);
    } else {
        command.arg("test");
    }
    command
        .args([
            "--locked",
            "-p",
            "terlan",
            "--lib",
            "--no-run",
            "--message-format=json-render-diagnostics",
            "--target-dir",
        ])
        .arg(&selection.target_directory);
    let mut artifacts = CargoArtifactStream::terlan_library(&root)?;
    artifacts.capture(&mut command, control, &mut |_| Ok(()), |_| Ok(()))?;
    executables.bind_declared_harness(artifacts.finish_terlan_library()?, control)?;
    let all = list(
        &environment,
        &executables,
        &selection.filter,
        false,
        selection.sanitizer,
        control,
    )?;
    let ignored = list(
        &environment,
        &executables,
        &selection.filter,
        true,
        selection.sanitizer,
        control,
    )?;
    if !ignored.is_subset(&all) {
        return Err(failure("inconsistent ignored-test inventory"));
    }
    let passed = all.difference(&ignored).cloned().collect::<BTreeSet<_>>();
    if passed.is_empty() {
        return Err(failure("filter selects zero runnable library tests"));
    }
    let expected = ExpectedTests {
        passed,
        ignored,
        filtered: 0,
    };
    let log = TestResultLog::create(&environment)?;
    let mut test = harness_command(&environment, &executables, selection.sanitizer, control)?;
    test.arg(&selection.filter)
        .args(["--test-threads=1", "--color=never", "--logfile"])
        .arg(log.path());
    control
        .run(&mut test, |_| Ok(()))
        .map_err(process_failure)?;
    let (bytes, identity) = log.read(control)?;
    inspect_workspace_log(&bytes, &expected, &identity)?;
    executables.verify(control)?;
    log.close()?;
    Ok(expected.passed.len())
}

fn list(
    environment: &ExecutionEnvironment,
    executables: &ExecutableBinding,
    filter: &str,
    ignored: bool,
    sanitizer: bool,
    control: ProcessControl<'_>,
) -> Result<BTreeSet<String>, PhaseFailure> {
    let mut command = harness_command(environment, executables, sanitizer, control)?;
    command.args(["--list", "--format=terse", filter]);
    if ignored {
        command.arg("--ignored");
    }
    let output = control
        .capture_stdout(&mut command, 16 * 1024 * 1024, |_| Ok(()))
        .map_err(process_failure)?;
    crate::test_inventory::parse(&output)
}

fn harness_command(
    environment: &ExecutionEnvironment,
    executables: &ExecutableBinding,
    sanitizer: bool,
    control: ProcessControl<'_>,
) -> Result<Command, PhaseFailure> {
    let mut command = environment.test_command(&executables.verify_harness(control)?);
    if sanitizer {
        // Preserve Cargo +toolchain dispatch for compiler subprocesses started
        // by the direct harness, not only its initial compilation.
        command.env("RUSTUP_TOOLCHAIN", "nightly-2026-07-16");
    }
    Ok(command)
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "library-filter-failed",
        detail: detail.to_string(),
    }
}
