//! Execute one Cargo-selected native test harness without exposing Cargo's JSON pipe.

use crate::cargo_harness_admission::DeclaredHarness;
use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::ExpectedTests;
use crate::workspace_native_storage::{executable_key, failure, read_json, write_new};
use crate::{file_identity, process_failure, PhaseFailure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use terlan_process_owner::ProcessControl;

/// Internal entry point; malformed requests fail before any selected executable is launched.
pub(super) fn main(arguments: impl Iterator<Item = OsString>) -> ExitCode {
    match execute(arguments.collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "[rust-test-suite] native runner {}: {}",
                error.outcome, error.detail
            );
            ExitCode::from(1)
        }
    }
}

fn execute(arguments: Vec<OsString>) -> Result<(), PhaseFailure> {
    let [directory, executable, rest @ ..] = arguments.as_slice() else {
        return Err(failure("native runner has no registry and executable"));
    };
    let directory = PathBuf::from(directory);
    if !directory.is_absolute()
        || !std::fs::symlink_metadata(&directory)
            .map_err(failure)?
            .is_dir()
    {
        return Err(failure(
            "native runner registry is not an absolute private directory",
        ));
    }
    let preflight = ProcessControl::new(Duration::from_secs(30));
    let (context, context_identity) = read_json(&directory.join("context.json"), preflight)?;
    let threads = context["threads"]
        .as_u64()
        .filter(|count| *count > 0)
        .ok_or_else(|| failure("invalid workspace thread count"))?;
    let timeout = context["timeout_seconds"]
        .as_u64()
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| failure("invalid workspace timeout"))?;
    let expected_arguments = [
        "--test-threads".to_string(),
        threads.to_string(),
        "--quiet".into(),
        "--color".into(),
        "never".into(),
    ];
    if context["schema"] != "terlan.workspace-native-context.v1"
        || rest
            != expected_arguments
                .iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
    {
        return Err(failure(
            "native runner received undeclared test selectors or context",
        ));
    }
    let control = ProcessControl::new(Duration::from_secs(timeout));
    let key = executable_key(Path::new(executable))?;
    wait_for(&directory.join("cargo.json"), control)?;
    let (cargo, _) = read_json(&directory.join("cargo.json"), control)?;
    let owner_pid = cargo["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| failure("native runner has no Cargo process owner"))?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let control = control.with_enclosing_process_group(owner_pid);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = owner_pid;
    let certificate_path = directory.join(format!("{key}.certificate.json"));
    wait_for(&certificate_path, control)?;
    let (certificate, certificate_identity) = read_json(&certificate_path, control)?;
    if certificate["schema"] != "terlan.workspace-native-certificate.v1" {
        return Err(failure("native runner has no declared certificate"));
    }
    let declaration: DeclaredHarness =
        serde_json::from_value(certificate["declaration"].clone()).map_err(failure)?;
    if executable_key(&declaration.executable)? != key {
        return Err(failure(
            "native runner executable does not match its certificate",
        ));
    }
    declaration.verify(control)?;
    let directory_now = std::env::current_dir().map_err(failure)?;
    declaration.verify_directory(&directory_now)?;
    let environment = ExecutionEnvironment::from_entries(std::env::vars_os(), &directory_now)?;
    let mut binding = ExecutableBinding::capture(
        &[("workspace-harness", declaration.executable.clone())],
        control,
    )?;
    if binding.json() != certificate["binding"] {
        return Err(failure(
            "native runner executable changed after authorization",
        ));
    }
    let mut launches = Vec::new();
    let mut observe = |role: &str, pid: u32| -> Result<(), String> {
        write_new(
            &directory.join(format!("{key}.{role}.json")),
            &json!({"pid":pid}),
        )
        .map_err(|error| error.detail)?;
        launches.push(json!({"role":role, "pid":pid}));
        Ok(())
    };
    observe("started", std::process::id()).map_err(failure)?;
    let delegated =
        crate::workspace_native::main_owner(&declaration.json(), &context["main_harness"])?;
    if certificate["delegated_main"] != delegated {
        return Err(failure(
            "native target delegation differs from its certificate",
        ));
    }
    if delegated {
        // Cargo selected this unit, but the suite's direct libtest phases own it.
        // Do not list or execute it a second time, and never report a test pass here.
        binding.verify(control)?;
        declaration.verify(control)?;
        return write_new(
            &directory.join(format!("{key}.complete.json")),
            &json!({
                "schema":"terlan.workspace-native-delegation.v1", "decision":"delegated",
                "owner":"main-library-phases", "certificate_identity":certificate_identity,
                "context_identity":context_identity, "launches":launches,
            }),
        );
    }
    let mut inventories = Vec::new();
    for role in ["all", "ignored"] {
        declaration.verify(control)?;
        let mut command =
            environment.command(&binding.verify_program("workspace-harness", control)?);
        command.args(["--list", "--format", "terse"]);
        if role == "ignored" {
            command.arg("--ignored");
        }
        let output = control
            .capture_stdout(&mut command, 16 * 1024 * 1024, |pid| observe(role, pid))
            .map_err(process_failure)?;
        inventories.push(crate::test_inventory::parse(&output)?);
    }
    let ignored = inventories.pop().expect("ignored inventory");
    let all = inventories.pop().expect("all inventory");
    if !ignored.is_subset(&all) || !ignored.is_empty() {
        return Err(failure(
            "workspace ignored tests require an explicit separate tier owner",
        ));
    }
    let expected = ExpectedTests {
        passed: all,
        ignored,
        filtered: 0,
    };
    declaration.verify(control)?;
    let mut command = environment.command(&binding.verify_program("workspace-harness", control)?);
    let result_log = directory.join(format!("{key}.libtest.log"));
    command.args(rest).arg("--logfile").arg(&result_log);
    let output = control
        .capture_stdout_observed(
            &mut command,
            16 * 1024 * 1024,
            |pid| observe("run", pid),
            |bytes| {
                // Test output must never masquerade as Cargo compiler-artifact JSON.
                let mut stderr = std::io::stderr().lock();
                stderr
                    .write_all(bytes)
                    .and_then(|()| stderr.flush())
                    .map_err(|error| error.to_string())
            },
        )
        .map_err(process_failure)?;
    output.outcome.map_err(process_failure)?;
    let mut identity = Sha256::new();
    let bytes = file_identity::read_hashed_file(
        &result_log,
        &mut identity,
        16 * 1024 * 1024,
        control,
        Instant::now(),
    )?;
    let evidence = crate::test_execution::inspect_workspace_log(
        &bytes,
        &expected,
        &file_identity::hex(identity),
    )?;
    declaration.verify(control)?;
    binding.verify(control)?;
    std::fs::remove_file(result_log).map_err(failure)?;
    write_new(
        &directory.join(format!("{key}.complete.json")),
        &json!({
            "schema":"terlan.workspace-native-completion.v1", "decision":"pass", "certificate_identity":certificate_identity,
            "context_identity":context_identity, "environment_identity":crate::execution_environment::identity(&environment.command(&declaration.executable)),
            "launches":launches, "test_execution":evidence.json(), "passed_names":expected.passed,
        }),
    )
}

/// Waits for an enclosing owner's atomic record within the existing process deadline.
pub(super) fn wait_for(path: &Path, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
    let started = Instant::now();
    loop {
        control.check(started).map_err(process_failure)?;
        if path.try_exists().map_err(failure)? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
#[path = "workspace_native_runner_test.rs"]
mod tests;
