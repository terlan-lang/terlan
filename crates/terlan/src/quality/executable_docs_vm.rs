use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::{render_failure, QualityResult};

/// User-facing documentation roots scanned for Terlan source examples.
const DOC_ROOTS: &[&str] = &["README.md", "docs", "std", "editors", "tree-sitter-terlan"];

/// Fence languages that are treated as Terlan source examples.
const TERLAN_FENCE_LANGUAGES: &[&str] = &["terlan", "terl"];

/// Removed or stale runtime-era terms that must not appear in Terlan examples.
const FORBIDDEN_EXAMPLE_TERMS: &[&str] = &[
    "std.beam",
    "beam-thin",
    "--target erlang",
    "--runtime beam",
    "Unit()",
    "None()",
];

/// Summary produced by the executable documentation VM gate.
///
/// Inputs:
/// - Counts measured from public Markdown files and fenced Terlan examples.
///
/// Output:
/// - Stable success metrics rendered by `terlan-quality`.
///
/// Transformation:
/// - Separates inventory counts from diagnostics so documentation drift can be
///   reported without hiding how much example surface was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableDocsVmSummary {
    pub markdown_file_count: usize,
    pub terlan_block_count: usize,
    pub complete_module_count: usize,
    pub fragment_count: usize,
}

/// One fenced Markdown code block containing Terlan source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerlanDocBlock {
    pub path: PathBuf,
    pub line: usize,
    pub language: String,
    pub body: String,
}

impl TerlanDocBlock {
    /// Returns whether this block is a complete Terlan module example.
    ///
    /// Inputs:
    /// - Fenced block body text.
    ///
    /// Output:
    /// - `true` for examples that can be written directly to `.terl` source.
    ///
    /// Transformation:
    /// - Uses the canonical module declaration prefix as the executable-docs
    ///   boundary. Smaller fragments stay inventoried but are not promoted to
    ///   compiler checks by this gate.
    pub(crate) fn is_complete_module(&self) -> bool {
        self.body.trim_start().starts_with("module ")
    }
}

/// Runs the VM executable documentation inventory gate.
///
/// Inputs:
/// - `root`: repository root containing user-facing docs.
///
/// Output:
/// - Success summary when Terlan examples are present and free of stale VM
///   pivot terms.
/// - Stable diagnostics when examples use retired runtime spellings or stale
///   file-extension aliases.
///
/// Transformation:
/// - Scans Markdown fences, classifies Terlan examples as complete modules or
///   fragments, rejects stale source text, and leaves actual complete-module
///   compilation to the paired exact `doc_test` release selector.
pub fn run_executable_docs_vm(root: &Path) -> QualityResult<ExecutableDocsVmSummary> {
    let markdown_files = collect_markdown_files(root)?;
    let blocks = collect_terlan_doc_blocks(root, &markdown_files)?;
    let diagnostics = validate_terlan_doc_blocks(&blocks);
    if !diagnostics.is_empty() {
        return Err(render_failure("executable-docs-vm", &diagnostics));
    }

    let complete_module_count = blocks
        .iter()
        .filter(|block| block.is_complete_module())
        .count();
    if complete_module_count == 0 {
        return Err(render_failure(
            "executable-docs-vm",
            &["no complete Terlan module examples found in public docs".to_string()],
        ));
    }

    Ok(ExecutableDocsVmSummary {
        markdown_file_count: markdown_files.len(),
        terlan_block_count: blocks.len(),
        complete_module_count,
        fragment_count: blocks.len().saturating_sub(complete_module_count),
    })
}

/// Collects user-facing Markdown files.
fn collect_markdown_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for relative in DOC_ROOTS {
        let path = root.join(relative);
        if path.is_file() {
            if is_markdown_path(&path) {
                files.push(PathBuf::from(relative));
            }
        } else if path.is_dir() {
            collect_markdown_files_in_dir(root, Path::new(relative), &mut files)?;
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Recursively collects Markdown files from one repository-relative directory.
fn collect_markdown_files_in_dir(
    root: &Path,
    relative: &Path,
    files: &mut Vec<PathBuf>,
) -> QualityResult<()> {
    let full_path = root.join(relative);
    for entry in fs::read_dir(&full_path)
        .map_err(|err| format!("{}: failed to read directory: {err}", relative.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "{}: failed to read directory entry: {err}",
                relative.display()
            )
        })?;
        let file_name = entry.file_name();
        let child = relative.join(file_name);
        let child_full_path = root.join(&child);
        if child_full_path.is_dir() {
            if should_skip_dir(&child) {
                continue;
            }
            collect_markdown_files_in_dir(root, &child, files)?;
        } else if child_full_path.is_file() && is_markdown_path(&child_full_path) {
            files.push(child);
        }
    }
    Ok(())
}

/// Extracts Terlan source fences from Markdown files.
pub(crate) fn collect_terlan_doc_blocks(
    root: &Path,
    markdown_files: &[PathBuf],
) -> QualityResult<Vec<TerlanDocBlock>> {
    let mut blocks = Vec::new();
    for relative in markdown_files {
        let text = fs::read_to_string(root.join(relative)).map_err(|err| {
            format!(
                "{}: failed to read markdown file: {err}",
                relative.display()
            )
        })?;
        blocks.extend(extract_terlan_doc_blocks(relative, &text));
    }
    Ok(blocks)
}

/// Extracts Terlan source fences from one Markdown document.
pub(crate) fn extract_terlan_doc_blocks(path: &Path, text: &str) -> Vec<TerlanDocBlock> {
    let mut blocks = Vec::new();
    let mut active: Option<(String, usize, Vec<String>)> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        if let Some((language, start_line, body)) = active.as_mut() {
            if trimmed.starts_with("```") {
                if is_terlan_fence_language(language) {
                    blocks.push(TerlanDocBlock {
                        path: path.to_path_buf(),
                        line: *start_line,
                        language: language.clone(),
                        body: body.join("\n"),
                    });
                }
                active = None;
            } else {
                body.push(line.to_string());
            }
            continue;
        }

        if let Some(language) = fence_language(trimmed) {
            active = Some((language.to_string(), line_number, Vec::new()));
        }
    }

    blocks
}

/// Validates Terlan documentation blocks against VM-era source rules.
pub(crate) fn validate_terlan_doc_blocks(blocks: &[TerlanDocBlock]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for block in blocks {
        if block.language == "tl" {
            diagnostics.push(format!(
                "{}:{}: stale Terlan fence language `tl`; use `terlan` or `terl`",
                block.path.display(),
                block.line
            ));
        }
        for term in FORBIDDEN_EXAMPLE_TERMS {
            if block.body.contains(term) {
                diagnostics.push(format!(
                    "{}:{}: Terlan example contains stale term `{term}`",
                    block.path.display(),
                    block.line
                ));
            }
        }
    }
    diagnostics
}

/// Returns the language after a Markdown fence opener.
fn fence_language(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line.strip_prefix("```")?;
    if rest.starts_with('`') {
        return None;
    }
    rest.split_whitespace().next()
}

/// Returns whether a Markdown fence language is Terlan source.
fn is_terlan_fence_language(language: &str) -> bool {
    TERLAN_FENCE_LANGUAGES.contains(&language) || language == "tl"
}

/// Returns whether a path is a Markdown document.
fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// Returns whether a documentation directory should be skipped.
fn should_skip_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), "_build" | "target" | "node_modules")
    })
}

#[cfg(test)]
#[path = "executable_docs_vm_test.rs"]
mod executable_docs_vm_test;
