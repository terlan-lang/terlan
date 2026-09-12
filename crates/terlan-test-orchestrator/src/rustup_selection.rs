//! Bounded active-name observation without inferring names from canonical roots.

use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::launch_ledger::LaunchLedger;
use crate::{process_failure, PhaseFailure, ValidationTier};
use std::path::{Path, PathBuf};
use terlan_process_owner::ProcessControl;

/// The active name and source Rustup exports to its proxy child.
pub(super) struct RustupSelection {
    /// Exact display name, preserving named versus absolute-path selection.
    pub(super) name: String,
    /// Rustup's stable source category, not the human-readable path/reason.
    pub(super) source: &'static str,
}

/// Observes the name Rustup exports to Cargo, preserving named versus path selection.
pub(super) fn observe(
    environment: &ExecutionEnvironment,
    executables: &ExecutableBinding,
    root: &Path,
    ledger: &mut LaunchLedger,
    control: ProcessControl<'_>,
) -> Result<RustupSelection, PhaseFailure> {
    ledger.execute(
        "Rustup active toolchain selection",
        ValidationTier::FastUnit,
        "rustup-tool-resolution",
        |launched| {
            let mut command =
                environment.test_command(&executables.verify_program("rustup", control)?);
            command
                .args(["show", "active-toolchain"])
                .env("RUSTUP_AUTO_INSTALL", "0");
            let output = control
                .capture_stdout(&mut command, 64 * 1024, launched)
                .map_err(process_failure)?;
            let home = home::env::rustup_home_with_env(environment).ok();
            parse(&output, root, home.as_deref())
        },
    )
}

fn parse(output: &[u8], root: &Path, home: Option<&Path>) -> Result<RustupSelection, PhaseFailure> {
    let output = std::str::from_utf8(output).map_err(failure)?;
    let line = output.strip_suffix('\n').unwrap_or(output);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.contains(['\n', '\r', '\0']) || !line.ends_with(')') {
        return Err(failure(
            "Rustup active selection must be one name and reason",
        ));
    }
    // Paths and reasons may contain spaces or parentheses. Accept exactly one
    // delimiter whose name maps to the already observed invocation root, not
    // a canonical equivalent which would discard an alias or path selection.
    let names = line
        .match_indices(" (")
        .filter_map(|(end, _)| {
            let name = &line[..end];
            let path = if Path::new(name).is_absolute() {
                PathBuf::from(name)
            } else if !name.is_empty() && !name.contains(['/', '\\']) {
                home?.join("toolchains").join(name)
            } else {
                return None;
            };
            (path == root).then_some(name)
        })
        .collect::<Vec<_>>();
    match names.as_slice() {
        [name] => {
            let reason = &line[name.len() + 2..line.len() - 1];
            let source = match reason {
                "default" => "default",
                "overridden by environment variable RUSTUP_TOOLCHAIN" => "env",
                "overridden by +toolchain on the command line" => "cli",
                reason
                    if reason.starts_with("directory override for '") && reason.ends_with('\'') =>
                {
                    "path-override"
                }
                reason if reason.starts_with("overridden by '") && reason.ends_with('\'') => {
                    "toolchain-file"
                }
                _ => return Err(failure("unrecognized Rustup active toolchain source")),
            };
            Ok(RustupSelection {
                name: (*name).to_owned(),
                source,
            })
        }
        _ => Err(failure(
            "Rustup active name does not uniquely match its admitted root",
        )),
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "rustup-selection-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "rustup_selection_test.rs"]
mod tests;
