//! Observe the original Rustdoc process without installing a doctest runtool.

use super::{failure, observer_name, Context};
use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::workspace_native_storage::{read_json, write_new};
use crate::PhaseFailure;
use serde_json::json;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

/// The private hardlink selects this mode without changing Rustdoc's argument vector.
pub(crate) fn is_observer(program: &std::ffi::OsStr) -> bool {
    Path::new(program)
        .file_name()
        .is_some_and(|name| name == observer_name().as_str())
}

/// Runs one independently admitted target and reports failures to Cargo.
pub(crate) fn main(program: OsString, arguments: impl Iterator<Item = OsString>) -> ExitCode {
    match execute(program, arguments.collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "[rust-test-suite] Rustdoc observer {}: {}",
                error.outcome, error.detail
            );
            ExitCode::FAILURE
        }
    }
}

fn execute(program: OsString, arguments: Vec<OsString>) -> Result<(), PhaseFailure> {
    let program = PathBuf::from(program);
    let directory = program
        .parent()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| failure("Rustdoc observer has no absolute registry"))?;
    if !std::fs::symlink_metadata(directory)
        .map_err(failure)?
        .is_dir()
        || arguments.len() > 4096
        || arguments
            .iter()
            .map(|arg| arg.as_encoded_bytes().len())
            .sum::<usize>()
            > 1024 * 1024
    {
        return Err(failure("invalid Rustdoc registry or excessive arguments"));
    }
    let (document, context_identity) = read_json(
        &directory.join("context.json"),
        ProcessControl::new(Duration::from_secs(30)),
    )?;
    let context: Context = serde_json::from_value(document.clone()).map_err(failure)?;
    if context.schema != "terlan.rustdoc-context.v1"
        || context.timeout_seconds == 0
        || context.targets.is_empty()
        || context.targets.len() > 256
        || context.rustdoc == program
    {
        return Err(failure("invalid Rustdoc context"));
    }
    let control = ProcessControl::new(Duration::from_secs(context.timeout_seconds));
    let current = std::env::current_dir().map_err(failure)?;
    let matches = context
        .targets
        .iter()
        .filter(|target| target.matches(&arguments, &current))
        .collect::<Vec<_>>();
    let [target] = matches.as_slice() else {
        return Err(failure(
            "Rustdoc invocation has no unique admitted workspace target",
        ));
    };
    let key = target.key()?;
    crate::workspace_native_runner::wait_for(&directory.join("cargo.json"), control)?;
    let (cargo, _) = read_json(&directory.join("cargo.json"), control)?;
    let pid = cargo["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| failure("Rustdoc observer has no enclosing Cargo owner"))?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let control = control.with_enclosing_process_group(pid);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let _ = pid;
    let started = json!({"role":"rustdoc-observer", "pid":std::process::id()});
    write_new(&directory.join(format!("{key}.started.json")), &started)?;
    let mut binding = ExecutableBinding::capture(&[("rustdoc", context.rustdoc.clone())], control)?;
    if binding.json() != context.rustdoc_binding {
        return Err(failure("Rustdoc executable changed after admission"));
    }
    let environment = ExecutionEnvironment::from_entries(std::env::vars_os(), &current)?;
    let mut command = environment.command(&binding.verify_program("rustdoc", control)?);
    command.args(arguments);
    let mut run = None;
    let output = control
        .capture_stdout_observed(
            &mut command,
            16 * 1024 * 1024,
            |pid| {
                let record = json!({"role":"rustdoc", "pid":pid});
                write_new(&directory.join(format!("{key}.run.json")), &record)
                    .map_err(|error| error.detail)?;
                run = Some(record);
                Ok(())
            },
            |bytes| {
                let mut stdout = std::io::stdout().lock();
                stdout
                    .write_all(bytes)
                    .and_then(|()| stdout.flush())
                    .map_err(|error| error.to_string())
            },
        )
        .map_err(crate::process_failure)?;
    output.outcome.map_err(crate::process_failure)?;
    let evidence = crate::doctest_output::inspect(&output.stdout)?;
    binding.verify(control)?;
    if read_json(&directory.join("context.json"), control)?.0 != document {
        return Err(failure("Rustdoc context changed during execution"));
    }
    let run = run.ok_or_else(|| failure("Rustdoc did not launch"))?;
    write_new(
        &directory.join(format!("{key}.complete.json")),
        &json!({
            "schema":"terlan.rustdoc-completion.v1", "context_identity":context_identity,
            "target":target, "launches":[started,run], "decision":"pass", "test_execution":evidence.json(),
        }),
    )
}
