use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::CliCommand;

#[derive(Debug, Serialize)]
struct InspectSnapshot {
    schema: &'static str,
    version: &'static str,
    runtime: &'static str,
    project: String,
    release_layout: ReleaseLayoutSnapshot,
}

#[derive(Debug, Serialize)]
struct ReleaseLayoutSnapshot {
    root: Option<String>,
    stdlib: bool,
    editor: bool,
    tree_sitter: bool,
}

/// Emits a deterministic snapshot of the public VM and installed release layout.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let project = match parse_args(&cmd.args) {
        Ok(project) => project,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let snapshot = InspectSnapshot {
        schema: "terlan.inspect-snapshot.v1",
        version: env!("CARGO_PKG_VERSION"),
        runtime: "vm",
        project: project.display().to_string(),
        release_layout: release_layout_snapshot(),
    };
    match serde_json::to_string_pretty(&snapshot) {
        Ok(rendered) => {
            println!("{rendered}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error[inspect_snapshot]: failed to render VM snapshot: {error}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<PathBuf, String> {
    let mut project = PathBuf::from(".");
    let mut saw_snapshot = false;
    for arg in args {
        match arg.as_str() {
            "--snapshot" if !saw_snapshot => saw_snapshot = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown terlc inspect option: {option}"));
            }
            path if project == Path::new(".") => project = PathBuf::from(path),
            path => {
                return Err(format!(
                    "terlc inspect accepts one project path, found `{path}`"
                ))
            }
        }
    }
    if !saw_snapshot {
        return Err("terlc inspect requires --snapshot".to_string());
    }
    fs::canonicalize(&project).map_err(|error| {
        format!(
            "error[inspect_snapshot]: cannot inspect `{}`: {error}",
            project.display()
        )
    })
}

fn release_layout_snapshot() -> ReleaseLayoutSnapshot {
    let root = crate::commands::release_layout::installed_share_root();
    ReleaseLayoutSnapshot {
        stdlib: root.as_ref().is_some_and(|path| path.join("std").is_dir()),
        editor: root
            .as_ref()
            .is_some_and(|path| path.join("editors/vscode/package.json").is_file()),
        tree_sitter: root
            .as_ref()
            .is_some_and(|path| path.join("tree-sitter-terlan/grammar.js").is_file()),
        root: root.map(|path| path.display().to_string()),
    }
}

#[cfg(test)]
#[path = "inspect_test.rs"]
#[cfg(test)]
mod inspect_test;
