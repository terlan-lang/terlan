use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::build::project_manifest;
use crate::CliCommand;

const PROJECT_MANIFEST_FILE: &str = "terlan.toml";
const SCRIPTS_DIR: &str = "scripts";

/// Discoverable runnable project script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectScriptEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) configured: bool,
}

/// Executes the `scripts` CLI command.
///
/// Inputs:
/// - `cmd`: parsed command with an optional project directory.
///
/// Output:
/// - Success when script discovery completes.
/// - Usage failure for malformed arguments.
/// - Failure when a configured alias points at an invalid runnable file.
///
/// Transformation:
/// - Discovers runnable `scripts/**/*.terls` files, merges `[scripts]` aliases
///   from `terlan.toml` when present, and prints a stable inventory.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let project_root = match parse_scripts_args(&cmd.args) {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    match discover_project_scripts(&project_root) {
        Ok(scripts) => {
            print_script_inventory(&scripts);
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Resolves a script name to a runnable project-local source path.
pub(crate) fn resolve_project_script(project_root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.trim().is_empty() {
        return Err("script name cannot be empty".to_string());
    }
    let scripts = discover_project_scripts(project_root)?;
    scripts
        .into_iter()
        .find(|script| script.name == name)
        .map(|script| script.path)
        .ok_or_else(|| {
            format!(
                "unknown project script `{name}`; run `terlc scripts {}` to list scripts",
                project_root.display()
            )
        })
}

/// Discovers runnable scripts for a project root.
pub(crate) fn discover_project_scripts(root: &Path) -> Result<Vec<ProjectScriptEntry>, String> {
    if !root.is_dir() {
        return Err(format!(
            "project directory does not exist: {}",
            root.display()
        ));
    }

    let mut entries = BTreeMap::new();
    for script in discover_convention_scripts(root)? {
        entries.insert(script.name.clone(), script);
    }
    for script in configured_scripts(root)? {
        if let Some(existing) = entries.get(&script.name) {
            if existing.path != script.path {
                return Err(format!(
                    "configured script alias `{}` conflicts with discovered script `{}`: {} vs {}",
                    script.name,
                    existing.name,
                    script.path.display(),
                    existing.path.display()
                ));
            }
        }
        entries.insert(script.name.clone(), script);
    }

    Ok(entries.into_values().collect())
}

/// Parses command-local `terlc scripts` arguments.
fn parse_scripts_args(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [] => Ok(PathBuf::from(".")),
        [arg] if matches!(arg.as_str(), "--help" | "-h") => {
            Err("terlc scripts [project-dir]".to_string())
        }
        [project] => Ok(PathBuf::from(project)),
        _ => Err("terlc scripts accepts at most one project directory".to_string()),
    }
}

/// Prints discovered scripts in a stable user-facing shape.
fn print_script_inventory(scripts: &[ProjectScriptEntry]) {
    if scripts.is_empty() {
        println!("No runnable scripts found.");
        return;
    }

    println!("Available scripts:");
    println!();
    for script in scripts {
        let marker = if script.configured {
            "configured"
        } else {
            "discovered"
        };
        println!("  {:<24} {} ({marker})", script.name, script.path.display());
    }
    println!();
    println!("Run with:");
    println!();
    println!("  terlc run script <name>");
}

/// Discovers first-class `scripts/**/*.terls` executable sources.
fn discover_convention_scripts(root: &Path) -> Result<Vec<ProjectScriptEntry>, String> {
    let scripts_root = root.join(SCRIPTS_DIR);
    if !scripts_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    collect_convention_scripts(root, &scripts_root, &mut entries)?;
    entries.sort_by(|left, right| left.name.cmp(&right.name).then(left.path.cmp(&right.path)));
    Ok(entries)
}

/// Recursively collects convention scripts.
fn collect_convention_scripts(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<ProjectScriptEntry>,
) -> Result<(), String> {
    let mut children = fs::read_dir(dir)
        .map_err(|err| {
            format!(
                "failed to read scripts directory `{}`: {err}",
                dir.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read scripts directory entry: {err}"))?;
    children.sort_by_key(|entry| entry.path());

    for child in children {
        let path = child.path();
        if path.is_dir() {
            collect_convention_scripts(root, &path, entries)?;
        } else if is_terlan_script_source(&path) {
            entries.push(ProjectScriptEntry {
                name: script_name_from_path(root, &path)?,
                path,
                configured: false,
            });
        }
    }
    Ok(())
}

/// Loads runnable aliases from `[scripts]`.
fn configured_scripts(root: &Path) -> Result<Vec<ProjectScriptEntry>, String> {
    let manifest_path = root.join(PROJECT_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return Ok(Vec::new());
    }

    let manifest = project_manifest::read_project_manifest(&manifest_path)?;
    let mut entries = Vec::new();
    for script in manifest.scripts {
        let path = root.join(&script.path);
        if !path.is_file() {
            return Err(format!(
                "configured script `{}` does not exist: {}",
                script.name,
                path.display()
            ));
        }
        if !is_terlan_script_source(&path) {
            return Err(format!(
                "configured script `{}` must use the `.terls` script extension: {}",
                script.name,
                path.display()
            ));
        }
        entries.push(ProjectScriptEntry {
            name: script.name,
            path,
            configured: true,
        });
    }
    Ok(entries)
}

/// Returns whether a path is a first-class Terlan script source.
fn is_terlan_script_source(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("terls")
}

/// Derives the default script name from a path below `scripts/`.
fn script_name_from_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        format!(
            "script path `{}` is not below project root `{}`",
            path.display(),
            root.display()
        )
    })?;
    let without_extension = relative.with_extension("");
    let mut parts = Vec::new();
    for component in without_extension.components() {
        let text = component.as_os_str().to_string_lossy();
        if text == SCRIPTS_DIR {
            continue;
        }
        parts.push(to_snake_case(&text));
    }
    Ok(parts.join("."))
}

/// Converts a path segment such as `SeedDatabase` to `seed_database`.
fn to_snake_case(value: &str) -> String {
    let mut out = String::new();
    let mut previous_was_separator = true;
    for ch in value.chars() {
        if matches!(ch, '-' | '_' | ' ' | '.') {
            if !previous_was_separator && !out.is_empty() {
                out.push('_');
            }
            previous_was_separator = true;
        } else if ch.is_ascii_uppercase() {
            if !previous_was_separator && !out.is_empty() {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        } else {
            out.push(ch);
            previous_was_separator = false;
        }
    }
    out
}

#[cfg(test)]
#[path = "scripts_test.rs"]
#[cfg(test)]
mod scripts_test;
