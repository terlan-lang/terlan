use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::CliCommand;

/// Parsed command-local arguments for `terlc clean`.
///
/// Inputs:
/// - Produced by `parse_clean_args` from command-local CLI arguments.
///
/// Output:
/// - Project directory whose generated outputs should be removed.
///
/// Transformation:
/// - Keeps command parsing separate from filesystem cleanup so tests can cover
///   each behavior directly.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanArgs {
    project_dir: PathBuf,
}

/// Relative output paths removed by `terlc clean`.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Stable list of project-local generated directories.
///
/// Transformation:
/// - Centralizes the cleanup contract so command logic, tests, and docs do not
///   duplicate path strings.
fn clean_output_paths() -> &'static [&'static str] {
    &["_build", ".terlan/tmp"]
}

/// Executes the `clean` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command with zero or one optional project directory.
///
/// Output:
/// - `ExitCode::SUCCESS` when every generated output is absent after cleanup.
/// - `ExitCode::from(2)` for malformed arguments.
/// - `ExitCode::from(1)` for filesystem deletion failures.
///
/// Transformation:
/// - Removes only compiler-owned generated output directories from the selected
///   project root and leaves source files, manifests, assets, tests, and user
///   configuration untouched.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let args = match parse_clean_args(&cmd.args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    match clean_project(&args.project_dir) {
        Ok(removed) => {
            if removed.is_empty() {
                println!("terlc clean: nothing to remove");
            } else {
                for path in removed {
                    println!("removed {}", path.display());
                }
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parses command-local arguments for `terlc clean`.
///
/// Inputs:
/// - `args`: raw command arguments after the top-level parser selected
///   `clean`.
///
/// Output:
/// - `Ok(CleanArgs)` for zero args or one project directory.
/// - `Err(message)` for unsupported flags or extra positional arguments.
///
/// Transformation:
/// - Defaults to the current directory and accepts a single explicit project
///   directory for editor and script usage.
fn parse_clean_args(args: &[String]) -> Result<CleanArgs, String> {
    if args.is_empty() {
        return Ok(CleanArgs {
            project_dir: PathBuf::from("."),
        });
    }
    if args.len() == 1 && !args[0].starts_with('-') {
        return Ok(CleanArgs {
            project_dir: PathBuf::from(&args[0]),
        });
    }
    if args.iter().any(|arg| arg.starts_with('-')) {
        return Err("terlc clean does not accept options yet".to_string());
    }
    Err("terlc clean accepts at most one project directory".to_string())
}

/// Removes generated output directories from a project.
///
/// Inputs:
/// - `project_dir`: project root or working directory to clean.
///
/// Output:
/// - Removed absolute or relative paths in deterministic cleanup order.
/// - `Err(message)` if a generated output path exists but cannot be removed.
///
/// Transformation:
/// - Joins each known generated path under `project_dir`, removes existing
///   directories/files, and skips absent paths without failing.
fn clean_project(project_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut removed = Vec::new();
    for relative in clean_output_paths() {
        let path = project_dir.join(relative);
        if !path.exists() {
            continue;
        }
        remove_output_path(&path)?;
        removed.push(path);
    }
    Ok(removed)
}

/// Removes one generated output path.
///
/// Inputs:
/// - `path`: compiler-owned generated path selected by `clean_project`.
///
/// Output:
/// - `Ok(())` when the path no longer exists.
/// - `Err(message)` when deletion fails.
///
/// Transformation:
/// - Deletes directories recursively and files directly, allowing future clean
///   targets to be either directory or file artifacts.
fn remove_output_path(path: &Path) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|err| format!("cannot inspect {}: {err}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|err| format!("cannot remove {}: {err}", path.display()))
    } else {
        fs::remove_file(path).map_err(|err| format!("cannot remove {}: {err}", path.display()))
    }
}

#[cfg(test)]
#[path = "clean_test.rs"]
mod clean_test;
