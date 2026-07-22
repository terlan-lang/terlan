use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

const MOBILE_MODULE_DIR: &str = "crates/terlan/src/mobile";
const COMPILER_MODULE_DIR: &str = "crates/terlan/src/compiler";
const BUILD_MOBILE_SOURCE: &str = "crates/terlan/src/commands/build/mobile.rs";
const COMPILER_MOD_SOURCE: &str = "crates/terlan/src/compiler/mod.rs";
const ALLOWED_COMPILER_MOBILE_HOOKS: &[&str] = &[
    "crates/terlan/src/compiler/typeck/mobile_bridge_validation.rs",
    "crates/terlan/src/compiler/typeck/mobile_bridge_validation_test.rs",
];

/// Summary produced by the mobile boundary gate.
///
/// Inputs:
/// - Mobile module files discovered under `crates/terlan/src/mobile`.
/// - Compiler-owned files checked for forbidden mobile implementation.
///
/// Output:
/// - Stable counts for the quality CLI.
///
/// Transformation:
/// - Records the module ownership boundary without coupling the CLI output to
///   individual file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobileBoundarySummary {
    pub mobile_file_count: usize,
    pub compiler_file_count: usize,
    pub allowed_hook_count: usize,
}

/// Runs the mobile source-boundary gate.
///
/// Inputs:
/// - `root`: repository root containing `crates/terlan/src`.
///
/// Output:
/// - Success when mobile implementation lives under `src/mobile`, compiler
///   module declarations do not own mobile implementation modules, and mobile
///   build planning imports from `crate::mobile`.
/// - Stable diagnostics when mobile implementation files are introduced under
///   `compiler` or build planning reaches into `crate::compiler::mobile_*`.
///
/// Transformation:
/// - Treats compiler/mobile separation as an executable ownership rule rather
///   than a roadmap note.
pub fn run_mobile_boundary(root: &Path) -> QualityResult<MobileBoundarySummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_mobile_module(root)?);
    let compiler_files = collect_rs_files(&root.join(COMPILER_MODULE_DIR))?;
    diagnostics.extend(validate_compiler_files(root, &compiler_files));
    diagnostics.extend(validate_source_text(root)?);

    if diagnostics.is_empty() {
        Ok(MobileBoundarySummary {
            mobile_file_count: collect_rs_files(&root.join(MOBILE_MODULE_DIR))?.len(),
            compiler_file_count: compiler_files.len(),
            allowed_hook_count: ALLOWED_COMPILER_MOBILE_HOOKS.len(),
        })
    } else {
        Err(render_failure("mobile-boundary", &diagnostics))
    }
}

/// Validates the mobile module exists with module docs.
fn validate_mobile_module(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for relative in [
        format!("{MOBILE_MODULE_DIR}/mod.rs"),
        format!("{MOBILE_MODULE_DIR}/README.md"),
    ] {
        if !root.join(&relative).is_file() {
            diagnostics.push(format!("{relative}: missing mobile module boundary file"));
        }
    }
    Ok(diagnostics)
}

/// Validates no mobile-owned implementation files live under compiler.
fn validate_compiler_files(root: &Path, files: &[PathBuf]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if ALLOWED_COMPILER_MOBILE_HOOKS.contains(&relative_text.as_str()) {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if file_name.starts_with("mobile_") || file_name.starts_with("reactive_ui_process") {
            diagnostics.push(format!(
                "{}: mobile implementation must live under {MOBILE_MODULE_DIR}",
                relative.display()
            ));
        }
    }
    diagnostics
}

/// Validates module declarations and imports describe the boundary.
fn validate_source_text(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    let compiler_mod = read_repo_text(root, COMPILER_MOD_SOURCE)?;
    for forbidden in [
        "mod mobile_",
        "pub(crate) mod mobile_",
        "mod reactive_ui_process",
        "pub(crate) mod reactive_ui_process",
    ] {
        if compiler_mod.contains(forbidden) {
            diagnostics.push(format!(
                "{COMPILER_MOD_SOURCE}: forbidden mobile module declaration `{forbidden}`"
            ));
        }
    }

    let build_mobile = read_repo_text(root, BUILD_MOBILE_SOURCE)?;
    if build_mobile.contains("crate::compiler::mobile_") {
        diagnostics.push(format!(
            "{BUILD_MOBILE_SOURCE}: mobile build planning must import from `crate::mobile`, not `crate::compiler`"
        ));
    }
    if !build_mobile.contains("crate::mobile::") {
        diagnostics.push(format!(
            "{BUILD_MOBILE_SOURCE}: mobile build planning must use `crate::mobile` boundary imports"
        ));
    }
    Ok(diagnostics)
}

/// Collects Rust files under one directory.
fn collect_rs_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_rs_files_recursive(root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects Rust files.
fn collect_rs_files_recursive(root: &Path, files: &mut Vec<PathBuf>) -> QualityResult<()> {
    for entry in fs::read_dir(root)
        .map_err(|err| format!("{}: failed to read directory: {err}", root.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files_recursive(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// Reads one repository text file.
fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read source file: {err}"))
}

#[cfg(test)]
#[path = "mobile_boundary_test.rs"]
mod mobile_boundary_test;
