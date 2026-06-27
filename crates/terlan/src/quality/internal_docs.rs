use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

/// Forbidden planning terms that must not appear in published docs paths.
const FORBIDDEN_NAME_PARTS: &[&str] = &["roadmap", "baseline", "checkpoint", "scratch", "research"];

/// Summary produced by the internal-docs check.
///
/// Inputs:
/// - `finding_count`: number of internal-looking docs paths found.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps scan metrics separate from failure diagnostics so success output is
///   stable and concise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalDocsSummary {
    pub finding_count: usize,
}

/// Published documentation path that looks internal.
///
/// Inputs:
/// - `path`: repository-relative path to an internal-looking document.
/// - `term`: forbidden term found in the path.
///
/// Output:
/// - Immutable finding for diagnostic rendering.
///
/// Transformation:
/// - Keeps the path and matched term together so maintainers can either delete
///   the file, move it to scratch documentation, or rename it as a
///   release-facing contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalDocFinding {
    pub path: PathBuf,
    pub term: String,
}

impl InternalDocFinding {
    /// Returns this finding as stable diagnostic text.
    ///
    /// Inputs:
    /// - Finding path and forbidden term.
    ///
    /// Output:
    /// - Human-readable diagnostic line.
    ///
    /// Transformation:
    /// - Formats one finding as `path: internal docs term ...`.
    pub fn render(&self) -> String {
        format!(
            "{}: internal docs term `{}` belongs outside published docs",
            self.path.display(),
            self.term
        )
    }
}

/// Runs the published-docs internal leakage check.
///
/// Inputs:
/// - `root`: repository root containing optional `docs/`.
///
/// Output:
/// - Success summary when no internal planning docs are present.
/// - Diagnostics for roadmap, baseline, checkpoint, scratch, or research paths
///   under published docs.
///
/// Transformation:
/// - Scans published documentation paths and rejects filenames or directories
///   containing internal planning terms.
pub fn run_internal_docs(root: &Path) -> QualityResult<InternalDocsSummary> {
    let findings = internal_doc_findings(&doc_paths(root)?);
    if !findings.is_empty() {
        let diagnostics = findings
            .iter()
            .map(InternalDocFinding::render)
            .collect::<Vec<_>>();
        return Err(render_failure("internal-docs", &diagnostics));
    }
    Ok(InternalDocsSummary { finding_count: 0 })
}

/// Returns published documentation files.
///
/// Inputs:
/// - `root`: repository root containing optional `docs/`.
///
/// Output:
/// - Repository-relative documentation file paths.
///
/// Transformation:
/// - Recursively walks `docs/` and ignores directories because file paths carry
///   the full directory context needed for matching.
fn doc_paths(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let docs = root.join("docs");
    if !docs.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    collect_doc_paths(root, &docs, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// Recursively collects documentation file paths.
///
/// Inputs:
/// - `root`: repository root used for relative paths.
/// - `dir`: current directory being scanned.
/// - `paths`: output accumulator.
///
/// Output:
/// - `Ok(())` when the directory tree was read.
/// - Diagnostic string when a directory entry or relative path fails.
///
/// Transformation:
/// - Traverses directories recursively and pushes only file paths.
fn collect_doc_paths(root: &Path, dir: &Path, paths: &mut Vec<PathBuf>) -> QualityResult<()> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read docs dir: {err}", dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("{}: failed to read docs entry: {err}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_doc_paths(root, &path, paths)?;
        } else if path.is_file() {
            let relative = path.strip_prefix(root).map_err(|err| {
                format!("{}: failed to relativize docs path: {err}", path.display())
            })?;
            paths.push(relative.to_path_buf());
        }
    }
    Ok(())
}

/// Returns internal-looking documentation path findings.
///
/// Inputs:
/// - `paths`: repository-relative documentation file paths.
///
/// Output:
/// - Findings for forbidden planning terms in path parts.
///
/// Transformation:
/// - Lowercases each path part and matches forbidden planning terms in the
///   filename or directory names.
pub(crate) fn internal_doc_findings(paths: &[PathBuf]) -> Vec<InternalDocFinding> {
    let mut findings = Vec::new();
    for path in paths {
        let lowered_parts = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();
        if let Some(term) = FORBIDDEN_NAME_PARTS
            .iter()
            .find(|term| lowered_parts.iter().any(|part| part.contains(*term)))
        {
            findings.push(InternalDocFinding {
                path: path.clone(),
                term: (*term).to_owned(),
            });
        }
    }
    findings
}

#[cfg(test)]
#[path = "internal_docs_test.rs"]
mod internal_docs_test;
