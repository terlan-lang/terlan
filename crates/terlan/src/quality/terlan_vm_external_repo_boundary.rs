use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::QualityResult;

/// Summary produced by the external VM repository boundary gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerlanVmExternalRepoBoundarySummary {
    pub checked_file_count: usize,
    pub reference_file_count: usize,
}

const BOUNDARY_DOC: &str = "docs/runtime/TERLAN_VM_EXTERNAL_REPO_BOUNDARY.md";

const REQUIRED_DOC_TERMS: &[&str] = &[
    "temporary history or migration source only",
    "not an active compiler dependency",
    "not required by default release checks",
    "reference-only migration evidence",
    "ambitious is a reference checklist only",
    "not a core dependency",
    "terlan vm owns scheduling",
    "active vm implementation lives inside `crates/terlan`",
    "same workspace package version",
    "same release artifact path",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const EXTERNAL_REFERENCE_PATTERNS: &[&str] =
    &["../terlan-vm", "cd terlan-vm", "path = \"../terlan-vm\""];

const ALLOWED_REFERENCE_PATHS: &[&str] = &[
    "Makefile",
    "crates/terlan/src/quality/terlan_vm_external_repo_boundary.rs",
    "crates/terlan/src/quality/terlan_vm_external_repo_boundary_test.rs",
    BOUNDARY_DOC,
];

/// Runs the external Terlan VM repository boundary gate.
///
/// Inputs:
/// - `root`: repository root containing the Makefile and runtime docs.
///
/// Output:
/// - Success summary when old external VM references are only documented
///   reference/migration material.
/// - Stable diagnostics when active code grows a new sibling-repository VM
///   dependency.
///
/// Transformation:
/// - Validates boundary documentation, scans repository text files for external
///   VM checkout references, and checks default release targets do not require
///   the old repository.
pub fn run_terlan_vm_external_repo_boundary(
    root: &Path,
) -> QualityResult<TerlanVmExternalRepoBoundarySummary> {
    let mut diagnostics = Vec::new();
    let boundary_doc = read_repo_text(root, BOUNDARY_DOC)?;
    diagnostics.extend(validate_boundary_doc_text(&boundary_doc));

    let files = repo_text_files(root)?;
    let references = collect_external_references(root, &files)?;
    diagnostics.extend(validate_reference_paths(&references));
    diagnostics.extend(validate_make_release_targets(root)?);
    diagnostics.extend(validate_same_crate_release_train(root)?);
    diagnostics.extend(validate_external_runtime_dependency_absence(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(TerlanVmExternalRepoBoundarySummary {
        checked_file_count: files.len() + 1,
        reference_file_count: references.len(),
    })
}

/// Validates the external VM boundary documentation text.
fn validate_boundary_doc_text(text: &str) -> Vec<String> {
    let normalized = normalize_text(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_DOC_TERMS {
        if !normalized.contains(&normalize_text(term)) {
            diagnostics.push(format!("missing external VM boundary term `{term}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(&normalize_text(placeholder)) {
            diagnostics.push(format!(
                "placeholder external VM boundary text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Collects files containing old external VM checkout references.
fn collect_external_references(root: &Path, files: &[PathBuf]) -> QualityResult<Vec<String>> {
    let mut references = Vec::new();
    for path in files {
        let text = fs::read_to_string(root.join(path))
            .map_err(|err| format!("{}: failed to read file: {err}", path.display()))?;
        if EXTERNAL_REFERENCE_PATTERNS
            .iter()
            .any(|pattern| text.contains(pattern))
        {
            references.push(path_to_slash(path));
        }
    }
    references.sort();
    references.dedup();
    Ok(references)
}

/// Validates external checkout references are constrained to allowed files.
fn validate_reference_paths(references: &[String]) -> Vec<String> {
    let allowed = ALLOWED_REFERENCE_PATHS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    references
        .iter()
        .filter(|path| !allowed.contains(path.as_str()))
        .map(|path| format!("unexpected external `terlan-vm` repository reference in `{path}`"))
        .collect()
}

/// Validates `terlc` and `terlan-vm` cannot drift into separate release paths.
fn validate_same_crate_release_train(root: &Path) -> QualityResult<Vec<String>> {
    let cargo = read_repo_text(root, "crates/terlan/Cargo.toml")?;
    let cli_mk = read_repo_text(root, "crates/terlan/cli.mk")?;
    let makefile = read_repo_text(root, "Makefile")?;
    let vm_cli = read_repo_text(root, "crates/terlan/src/vm/cli.rs")?;
    let mut diagnostics = Vec::new();

    if !cargo.contains("version.workspace = true") {
        diagnostics.push("compiler crate must inherit the workspace version".to_string());
    }
    if !cargo.contains("name = \"terlc\"") || !cargo.contains("path = \"src/main.rs\"") {
        diagnostics.push("compiler crate must own the `terlc` binary".to_string());
    }
    if !cargo.contains("name = \"terlan-vm\"") || !cargo.contains("path = \"src/vm/main.rs\"") {
        diagnostics.push("compiler crate must own the `terlan-vm` binary".to_string());
    }
    if !vm_cli.contains("env!(\"CARGO_PKG_VERSION\")") {
        diagnostics.push("terlan-vm version must come from the shared Cargo package".to_string());
    }

    let compiler_bootstrap = make_target_body(&makefile, "terlan-compiler-bootstrap");
    if !compiler_bootstrap.contains("--bin terlc --bin terlan-vm") {
        diagnostics
            .push("compiler bootstrap must build `terlc` and `terlan-vm` together".to_string());
    }

    let cli_test_full = make_target_body(&cli_mk, "cli-test-full");
    if !cli_test_full.contains("--workspace")
        && !cli_test_full.contains("--bin terlc --bin terlan-vm")
    {
        diagnostics.push(
            "cli-test-full must test the workspace or both `terlc` and `terlan-vm`".to_string(),
        );
    }

    let release_artifact = make_target_body(&cli_mk, "cli-release-artifact-current");
    if !release_artifact.contains("--bin terlc --bin terlan-vm") {
        diagnostics.push(
            "release artifact build must include `terlc` and `terlan-vm` together".to_string(),
        );
    }

    let upgrade_local = make_target_body(&makefile, "upgrade-local");
    if !upgrade_local.contains("--bin terlc --bin terlan-vm") {
        diagnostics.push("upgrade-local must build `terlc` and `terlan-vm` together".to_string());
    }
    if !upgrade_local.contains("target/release/terlan-vm") {
        diagnostics.push("upgrade-local must install `terlan-vm` beside `terlc`".to_string());
    }

    let test_release = make_target_body(&makefile, "test-release");
    if !test_release.contains("terlan-release-train-check") {
        diagnostics.push("test-release must include the Terlan release-train gate".to_string());
    }

    Ok(diagnostics)
}

/// Validates third-party OTP-like runtimes remain references, not dependencies.
fn validate_external_runtime_dependency_absence(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for relative in ["Cargo.toml", "Cargo.lock", "crates/terlan/Cargo.toml"] {
        let text = read_repo_text(root, relative)?;
        if cargo_metadata_declares_ambitious(&text) {
            diagnostics.push(format!(
                "`{relative}` must not declare `ambitious`; it is a reference checklist only"
            ));
        }
    }
    Ok(diagnostics)
}

/// Returns whether Cargo metadata declares the `ambitious` crate.
fn cargo_metadata_declares_ambitious(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == "name = \"ambitious\""
            || trimmed.starts_with("ambitious =")
            || trimmed.starts_with("ambitious.")
            || trimmed.starts_with("\"ambitious\" =")
    })
}

/// Validates default release targets do not depend on the old external VM repo.
fn validate_make_release_targets(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read_repo_text(root, "Makefile")?;
    let mut diagnostics = Vec::new();
    for target in ["check", "test", "test-release"] {
        let body = make_target_body(&makefile, target);
        if body.contains("../terlan-vm") {
            diagnostics.push(format!(
                "`make {target}` must not require the old external `terlan-vm` repository"
            ));
        }
    }
    if makefile.contains("erlang-modernization-em0-full-compatibility-gate")
        || makefile.contains("TERLAN_RUN_FULL_OTP_COMPATIBILITY")
    {
        diagnostics.push("full OTP compatibility Make gate must be removed".to_string());
    }
    Ok(diagnostics)
}

/// Extracts the body of one simple Make target.
fn make_target_body(makefile: &str, target: &str) -> String {
    let target_prefix = format!("{target}:");
    let mut body = String::new();
    let mut in_target = false;
    for line in makefile.lines() {
        if in_target && !line.starts_with('\t') && line.contains(':') && !line.starts_with('.') {
            break;
        }
        if in_target {
            body.push_str(line);
            body.push('\n');
        } else if line.starts_with(&target_prefix) {
            in_target = true;
            body.push_str(line);
            body.push('\n');
        }
    }
    body
}

/// Returns repository text files that are cheap and safe to scan.
fn repo_text_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_text_files(root, Path::new("."), &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collects text-like repository files.
fn collect_text_files(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> QualityResult<()> {
    let path = root.join(relative);
    for entry in fs::read_dir(&path)
        .map_err(|err| format!("{}: failed to read dir: {err}", path.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read dir entry: {err}", path.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_entry(&name) {
            continue;
        }
        let child_relative = relative.join(name.as_ref());
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "{}: failed to read file type: {err}",
                child_relative.display()
            )
        })?;
        if file_type.is_dir() {
            collect_text_files(root, &child_relative, files)?;
        } else if file_type.is_file() && is_scanned_text_file(&child_relative) {
            files.push(
                child_relative
                    .strip_prefix(".")
                    .unwrap_or(&child_relative)
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

/// Returns whether a repository entry should be skipped by the scanner.
fn should_skip_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "dist" | "_build" | "node_modules" | ".DS_Store"
    )
}

/// Returns whether the file extension is treated as text for this gate.
fn is_scanned_text_file(path: &Path) -> bool {
    if let Some("Makefile" | "Cargo.toml") = path.file_name().and_then(|name| name.to_str()) {
        return true;
    }
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("rs" | "md" | "mk" | "toml" | "sh" | "py" | "json" | "yml" | "yaml")
    )
}

/// Reads a repository-relative text file.
fn read_repo_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

/// Normalizes text for term checks.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts a path to slash-separated repository-relative text.
fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders external VM boundary diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[terlan-vm-external-repo-boundary] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "terlan_vm_external_repo_boundary_test.rs"]
#[cfg(test)]
mod terlan_vm_external_repo_boundary_test;
