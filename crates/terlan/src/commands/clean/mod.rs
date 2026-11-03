use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::build::project_manifest;
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

/// Returns compiler summary artifacts stored in project-owned cache roots.
fn clean_summary_output_paths(project_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut summaries = Vec::new();
    for root in clean_summary_roots(project_dir)? {
        summaries.extend(summary_output_paths(&root)?);
    }
    summaries.sort();
    Ok(summaries)
}

/// Returns the project root and every manifest-configured source root.
fn clean_summary_roots(project_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut roots = vec![project_dir.to_path_buf()];
    let manifest_path = project_dir.join("terlan.toml");
    if !manifest_path.is_file() {
        return Ok(roots);
    }

    let manifest = project_manifest::read_project_manifest(&manifest_path)?;
    let canonical_project = fs::canonicalize(project_dir).map_err(|error| {
        format!(
            "cannot resolve clean project directory {}: {error}",
            project_dir.display()
        )
    })?;
    for configured in manifest.source_roots {
        let source_root = project_dir.join(configured);
        let canonical_source = match fs::canonicalize(&source_root) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "cannot resolve clean source root {}: {error}",
                    source_root.display()
                ));
            }
        };
        if !canonical_source.starts_with(&canonical_project) {
            return Err(format!(
                "refusing to clean source root outside project: {}",
                source_root.display()
            ));
        }
        roots.push(source_root);
    }
    Ok(roots)
}

/// Returns summary files stored directly in one root's compiler cache.
fn summary_output_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let cache = root.join(".terlan");
    let metadata = match fs::symlink_metadata(&cache) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect {}: {error}", cache.display())),
    };
    if !metadata.file_type().is_dir() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();
    for entry in
        fs::read_dir(&cache).map_err(|error| format!("cannot read {}: {error}", cache.display()))?
    {
        let entry = entry.map_err(|error| {
            format!(
                "cannot read directory entry in {}: {error}",
                cache.display()
            )
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (file_type.is_file() || file_type.is_symlink())
            && (name.ends_with(".typi") || name.contains(".typi."))
        {
            summaries.push(entry.path());
        }
    }
    Ok(summaries)
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
///   project root and configured source roots, leaving source files, manifests,
///   assets, tests, and user configuration untouched.
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
    for path in clean_summary_output_paths(project_dir)? {
        fs::remove_file(&path).map_err(|err| format!("cannot remove {}: {err}", path.display()))?;
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
#[cfg(test)]
mod clean_test;
