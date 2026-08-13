use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

/// Forbidden planning terms that must not appear in published docs paths.
const FORBIDDEN_NAME_PARTS: &[&str] = &[
    "roadmap",
    "archive",
    "baseline",
    "checkpoint",
    "scratch",
    "research",
    "inventory",
    "evidence",
];

/// Source-controlled allowlist for documentation copied into release archives.
const PUBLISHABLE_MANIFEST: &str = "docs/release/PUBLISHABLE_DOCUMENTATION.tsv";

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
/// - Reads the explicit staged-documentation manifest and rejects malformed,
///   unsorted, duplicate, missing, or out-of-tree entries.
fn doc_paths(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let manifest_path = root.join(PUBLISHABLE_MANIFEST);
    let source = fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "{}: failed to read publishable documentation manifest: {err}",
            manifest_path.display()
        )
    })?;
    let mut paths = Vec::new();
    let mut previous: Option<&str> = None;
    for (index, line) in source.lines().enumerate() {
        if index == 0 {
            if line != "path" {
                return Err(format!(
                    "{}: expected exact `path` header",
                    manifest_path.display()
                ));
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !line.starts_with("docs/") || line.contains('\t') {
            return Err(format!(
                "{}:{}: invalid publishable documentation path `{line}`",
                manifest_path.display(),
                index + 1
            ));
        }
        if previous.is_some_and(|value| value >= line) {
            return Err(format!(
                "{}:{}: paths must be unique and bytewise sorted",
                manifest_path.display(),
                index + 1
            ));
        }
        let relative = PathBuf::from(line);
        if !root.join(&relative).is_file() {
            return Err(format!(
                "{}:{}: publishable document is missing: {line}",
                manifest_path.display(),
                index + 1
            ));
        }
        paths.push(relative);
        previous = Some(line);
    }
    if paths.is_empty() {
        return Err(format!(
            "{}: publishable documentation manifest is empty",
            manifest_path.display()
        ));
    }
    Ok(paths)
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
#[cfg(test)]
mod internal_docs_test;
