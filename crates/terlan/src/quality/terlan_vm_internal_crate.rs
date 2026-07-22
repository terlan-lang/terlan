use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Summary produced by the Terlan VM internal crate-shape gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerlanVmInternalCrateSummary {
    pub checked_file_count: usize,
}

const REQUIRED_VM_README_TERMS: &[&str] = &[
    "internal compiler/runtime implementation detail",
    "not a separate public vm distribution",
    "reuses the formal compiler pipeline",
    "runtime::vm",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const FORBIDDEN_WORKSPACE_TERMS: &[&str] = &[
    "\"crates/terlan-vm\"",
    "\"crates/terlan_vm\"",
    "\"crates/terlan_runtime_vm\"",
];

/// Runs the Terlan VM internal crate-shape gate.
///
/// Inputs:
/// - `root`: repository root containing workspace and crate manifests.
///
/// Output:
/// - Success summary when `terlan-vm` is built from the compiler crate.
/// - Stable diagnostics when a separate VM crate or public-product wording
///   appears.
///
/// Transformation:
/// - Validates workspace membership, the `terlan-vm` binary path, and VM README
///   ownership wording without invoking the VM.
pub fn run_terlan_vm_internal_crate(root: &Path) -> QualityResult<TerlanVmInternalCrateSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_workspace_manifest(root)?);
    diagnostics.extend(validate_terlan_crate_manifest(root)?);
    diagnostics.extend(validate_vm_main(root)?);
    diagnostics.extend(validate_vm_readme(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(TerlanVmInternalCrateSummary {
        checked_file_count: 4,
    })
}

/// Validates the root workspace does not grow a separate VM crate.
fn validate_workspace_manifest(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join("Cargo.toml");
    let text = read_text(&path)?;
    let mut diagnostics = Vec::new();
    if !text.contains("\"crates/terlan\"") {
        diagnostics.push("workspace must include `crates/terlan`".to_string());
    }
    for term in FORBIDDEN_WORKSPACE_TERMS {
        if text.contains(term) {
            diagnostics.push(format!(
                "workspace must not include separate VM crate {term}"
            ));
        }
    }
    Ok(diagnostics)
}

/// Validates the compiler crate owns the `terlan-vm` binary.
fn validate_terlan_crate_manifest(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join("crates/terlan/Cargo.toml");
    let text = read_text(&path)?;
    let mut diagnostics = Vec::new();
    if !text.contains("name = \"terlan-vm\"") {
        diagnostics.push("compiler crate must declare `terlan-vm` binary".to_string());
    }
    if !text.contains("path = \"src/vm/main.rs\"") {
        diagnostics.push("`terlan-vm` binary must use `src/vm/main.rs`".to_string());
    }
    if !root.join("crates/terlan/src/vm/main.rs").exists() {
        diagnostics.push("`crates/terlan/src/vm/main.rs` is missing".to_string());
    }
    Ok(diagnostics)
}

/// Validates the standalone VM entrypoint does not import the Erlang backend.
fn validate_vm_main(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join("crates/terlan/src/vm/main.rs");
    let text = read_text(&path)?;
    Ok(validate_vm_main_text(&text))
}

/// Validates VM entrypoint text for active backend imports.
fn validate_vm_main_text(text: &str) -> Vec<String> {
    let forbidden_fragments = [
        "../backends/mod.rs",
        "pub mod backends",
        "terlan_erlang",
        "backends::erlang",
    ];
    forbidden_fragments
        .iter()
        .filter(|fragment| text.contains(**fragment))
        .map(|fragment| {
            format!("`terlan-vm` entrypoint must not import Erlang backend fragment `{fragment}`")
        })
        .collect()
}

/// Validates VM ownership wording.
fn validate_vm_readme(root: &Path) -> QualityResult<Vec<String>> {
    let path = root.join("crates/terlan/src/vm/README.md");
    let text = read_text(&path)?;
    Ok(validate_vm_readme_text(&text))
}

/// Validates VM README text.
fn validate_vm_readme_text(text: &str) -> Vec<String> {
    let normalized = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut diagnostics = Vec::new();
    for term in REQUIRED_VM_README_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!("missing VM README ownership term `{term}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder VM README ownership text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Reads one text file for validation.
fn read_text(path: &Path) -> QualityResult<String> {
    fs::read_to_string(path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

/// Renders VM internal-shape diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[terlan-vm-internal-crate] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "terlan_vm_internal_crate_test.rs"]
mod terlan_vm_internal_crate_test;
