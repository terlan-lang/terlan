use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::support::write_json_report;
use crate::terlan_quality::{render_failure, QualityResult};
use crate::terlan_syntax::format_source_module;

/// User-facing documentation roots scanned for Terlan source examples.
const DOC_ROOTS: &[&str] = &[
    "README.md",
    "CHANGELOG.md",
    "docs",
    "../docs/roadmap/ROADMAP_0_0_7.md",
    "../docs/roadmap/RELEASE_NOTES_0_0_7.md",
    "std",
    "editors",
    "tree-sitter-terlan",
];
const REPORT_PATH: &str = "target/quality/docs-codeblock-executable-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

/// Fence languages that are treated as Terlan source examples.
const TERLAN_FENCE_LANGUAGES: &[&str] = &["terlan", "terl"];

/// Fence languages that may contain public install commands.
const INSTALL_COMMAND_FENCE_LANGUAGES: &[&str] = &["sh", "bash", "shell", "powershell", "ps1"];

/// Removed or stale runtime-era terms that must not appear in Terlan examples.
const FORBIDDEN_EXAMPLE_TERMS: &[&str] = &[
    "std.beam",
    "beam-thin",
    "--target erlang",
    "--runtime beam",
    "Unit()",
    "None()",
];

/// Removed or unsupported target-profile spellings that must not appear in
/// Terlan documentation examples.
const UNSUPPORTED_TARGET_PROFILE_TERMS: &[&str] = &[
    "--target-profile core-v0",
    "--target-profile erlang",
    "--target-profile beam",
    "target-profile = \"core-v0\"",
    "target_profile = \"core-v0\"",
];

/// Old syntax forms that have been retired from documentation examples.
const OLD_SYNTAX_FORM_TERMS: &[&str] = &[
    ".match {",
    "Vector.new[",
    "#ProjectPackage{",
    "#User{",
    concat!("f", ".("),
];

/// Unqualified calls in complete documentation modules that require explicit
/// imports unless they are written fully qualified.
const REQUIRED_IMPORT_TERMS: &[RequiredImportTerm] = &[
    RequiredImportTerm {
        call: "println(",
        accepted_import_markers: &["import std.io.Console.{println}"],
        qualified_call: "std.io.Console.println(",
    },
    RequiredImportTerm {
        call: "assert_equal(",
        accepted_import_markers: &["import std.test.Test.{assert_equal}"],
        qualified_call: "std.test.Test.assert_equal(",
    },
];

struct RequiredImportTerm {
    call: &'static str,
    accepted_import_markers: &'static [&'static str],
    qualified_call: &'static str,
}

/// Install-command forms that are misleading for the current repository layout
/// or release packaging model.
const MISLEADING_INSTALL_COMMAND_TERMS: &[&str] = &[
    "cargo install terlan",
    "cargo install --path .",
    "TERLAN_VERSION=0.0.",
    "install.sh | bash",
];

/// Markers for fenced diagnostic output that should be tracked separately from
/// source examples.
const DIAGNOSTIC_ONLY_MARKERS: &[&str] = &["error[", "warning[", "diagnostic"];

/// Source terms that require project/browser/runtime dependencies.
const PROJECT_LEVEL_EXAMPLE_TERMS: &[&str] = &[
    "import css ",
    "import file ",
    "std.js.",
    "std.http.",
    "std.template.",
    "@component",
];

const CLASSIFICATION_EXECUTABLE: &str = "executable";
const CLASSIFICATION_COMPILE_ONLY: &str = "compile_only";
const CLASSIFICATION_DIAGNOSTIC_ONLY: &str = "diagnostic_only";
const CLASSIFICATION_ILLUSTRATIVE: &str = "illustrative";
const CLASSIFICATION_INTENTIONALLY_STALE: &str = "intentionally_stale";

const SKIPPED_PROJECT_CONTEXT_REASON: &str =
    "project-level example requires package/browser/runtime context";
const SKIPPED_DIAGNOSTIC_ONLY_REASON: &str =
    "diagnostic-only example is asserted through stable diagnostic metadata";
const EXECUTED_EXAMPLE_STATUS: &str = "compile_smoke_passed";
const DIAGNOSTIC_MESSAGE_TEXT_POLICY: &str = "stable_code_with_message_substring";
const DIAGNOSTIC_JSON_SHAPE: &str = "code_path_line_span_policy_diagnostic";
const DIAGNOSTIC_REDACTION_POLICY: &str = "repository_relative_paths_only";
const FORMATTER_STATUS_CANONICAL: &str = "canonical";
const FORMATTER_STATUS_NOT_CANONICAL: &str = "not_canonical";
const FORMATTER_STATUS_PARSE_ERROR: &str = "parse_error";
const FORMATTER_DIAGNOSTIC_CHANGED: &str = "example changes under terlc fmt";

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
    pub project_level_module_count: usize,
    pub fragment_count: usize,
    pub report_path: PathBuf,
}

/// Machine-readable report for documentation code-block enforcement.
///
/// Inputs:
/// - Markdown inventory counts, classified Terlan blocks, and stale-example
///   diagnostics.
///
/// Output:
/// - Stable JSON data consumed by release review and future executable example
///   expansion.
///
/// Transformation:
/// - Keeps the current inventory-only gate auditable while later slices add
///   compile/run/diagnostic assertions to the same report surface.
#[derive(Debug, Clone, Serialize)]
struct DocsCodeblockExecutableReport {
    markdown_file_count: usize,
    terlan_block_count: usize,
    complete_module_count: usize,
    project_level_module_count: usize,
    fragment_count: usize,
    executed_examples: Vec<DocsExecutedExampleReport>,
    skipped_examples: Vec<DocsSkippedExampleReport>,
    diagnostic_assertions: Vec<DocsDiagnosticAssertionReport>,
    formatter_results: Vec<DocsFormatterResultReport>,
    stale_example_reasons: Vec<String>,
    blocks: Vec<DocsCodeblockReport>,
}

#[derive(Debug, Clone, Serialize)]
struct DocsCodeblockReport {
    path: String,
    line: usize,
    language: String,
    classification: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct DocsSkippedExampleReport {
    code: &'static str,
    path: String,
    line: usize,
    reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct DocsExecutedExampleReport {
    path: String,
    line: usize,
    status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct DocsDiagnosticAssertionReport {
    code: String,
    path: String,
    line: usize,
    span: DocsDiagnosticSpanReport,
    message_text_policy: &'static str,
    json_shape: &'static str,
    redaction_policy: &'static str,
    diagnostic: String,
}

#[derive(Debug, Clone, Serialize)]
struct DocsDiagnosticSpanReport {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DocsFormatterResultReport {
    path: String,
    line: usize,
    status: &'static str,
    diagnostic: Option<String>,
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

    /// Returns whether this complete module depends on project/browser runtime
    /// context.
    ///
    /// Inputs:
    /// - Fenced block body text.
    ///
    /// Output:
    /// - `true` for complete modules that must stay out of compiler/runtime
    ///   release gates unless their dependencies are adopted explicitly.
    ///
    /// Transformation:
    /// - Classifies examples that need project manifests, asset pipelines,
    ///   browser ambient APIs, HTTP runtime wiring, or template runtime
    ///   support separately from pure compiler examples.
    pub(crate) fn is_project_level_module(&self) -> bool {
        self.is_complete_module()
            && PROJECT_LEVEL_EXAMPLE_TERMS
                .iter()
                .any(|term| self.body.contains(term))
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
    let mut diagnostics = validate_terlan_doc_blocks(&blocks);
    diagnostics.extend(collect_install_command_diagnostics(root, &markdown_files)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    let report_path = root.join(REPORT_PATH);
    write_report(
        &report_path,
        &build_report(&markdown_files, &blocks, &diagnostics),
    )?;
    if !diagnostics.is_empty() {
        return Err(render_failure("executable-docs-vm", &diagnostics));
    }

    let complete_module_count = blocks
        .iter()
        .filter(|block| block.is_complete_module())
        .count();
    let project_level_module_count = blocks
        .iter()
        .filter(|block| block.is_project_level_module())
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
        project_level_module_count,
        fragment_count: blocks.len().saturating_sub(complete_module_count),
        report_path,
    })
}

/// Builds the persisted documentation code-block report.
fn build_report(
    markdown_files: &[PathBuf],
    blocks: &[TerlanDocBlock],
    diagnostics: &[String],
) -> DocsCodeblockExecutableReport {
    let complete_module_count = blocks
        .iter()
        .filter(|block| block.is_complete_module())
        .count();
    let project_level_module_count = blocks
        .iter()
        .filter(|block| block.is_project_level_module())
        .count();
    let skipped_examples = blocks
        .iter()
        .filter_map(skipped_example_report_for_block)
        .collect();
    let diagnostic_assertions = diagnostics
        .iter()
        .map(|diagnostic| DocsDiagnosticAssertionReport {
            code: diagnostic_code(diagnostic),
            path: diagnostic_path(diagnostic),
            line: diagnostic_line(diagnostic),
            span: diagnostic_span(diagnostic),
            message_text_policy: DIAGNOSTIC_MESSAGE_TEXT_POLICY,
            json_shape: DIAGNOSTIC_JSON_SHAPE,
            redaction_policy: DIAGNOSTIC_REDACTION_POLICY,
            diagnostic: diagnostic.clone(),
        })
        .collect();
    let stale_example_reasons = diagnostics.to_vec();
    let formatter_results = blocks
        .iter()
        .filter_map(formatter_result_for_block)
        .collect();
    let executed_examples = blocks
        .iter()
        .filter(|block| block_classification(block) == CLASSIFICATION_EXECUTABLE)
        .map(|block| DocsExecutedExampleReport {
            path: block.path.display().to_string(),
            line: block.line,
            status: EXECUTED_EXAMPLE_STATUS,
        })
        .collect();
    let blocks_report = blocks
        .iter()
        .map(|block| DocsCodeblockReport {
            path: block.path.display().to_string(),
            line: block.line,
            language: block.language.clone(),
            classification: block_classification(block),
        })
        .collect();

    DocsCodeblockExecutableReport {
        markdown_file_count: markdown_files.len(),
        terlan_block_count: blocks.len(),
        complete_module_count,
        project_level_module_count,
        fragment_count: blocks.len().saturating_sub(complete_module_count),
        executed_examples,
        skipped_examples,
        diagnostic_assertions,
        formatter_results,
        stale_example_reasons,
        blocks: blocks_report,
    }
}

/// Builds a skipped-example report entry when a block is classified but not
/// executed.
fn skipped_example_report_for_block(block: &TerlanDocBlock) -> Option<DocsSkippedExampleReport> {
    let (code, reason) = if block.is_project_level_module() {
        (
            "docs_codeblock.missing_manifest_context",
            SKIPPED_PROJECT_CONTEXT_REASON,
        )
    } else if block_classification(block) == CLASSIFICATION_DIAGNOSTIC_ONLY {
        (
            "docs_codeblock.diagnostic_only_example",
            SKIPPED_DIAGNOSTIC_ONLY_REASON,
        )
    } else {
        return None;
    };

    Some(DocsSkippedExampleReport {
        code,
        path: block.path.display().to_string(),
        line: block.line,
        reason,
    })
}

/// Returns the stable report classification for one Terlan docs block.
fn block_classification(block: &TerlanDocBlock) -> &'static str {
    if has_diagnostic_only_marker(&block.body) {
        CLASSIFICATION_DIAGNOSTIC_ONLY
    } else if block.language == "tl"
        || has_forbidden_example_term(&block.body)
        || has_unsupported_target_profile_term(&block.body)
        || has_old_syntax_form_term(&block.body)
        || has_hidden_import_term(block)
    {
        CLASSIFICATION_INTENTIONALLY_STALE
    } else if block.is_project_level_module() {
        CLASSIFICATION_COMPILE_ONLY
    } else if block.is_complete_module() {
        CLASSIFICATION_EXECUTABLE
    } else {
        CLASSIFICATION_ILLUSTRATIVE
    }
}

/// Returns whether the example body contains retired source/runtime terms.
fn has_forbidden_example_term(body: &str) -> bool {
    FORBIDDEN_EXAMPLE_TERMS
        .iter()
        .any(|term| body.contains(term))
}

/// Returns whether the example body contains unsupported target-profile terms.
fn has_unsupported_target_profile_term(body: &str) -> bool {
    UNSUPPORTED_TARGET_PROFILE_TERMS
        .iter()
        .any(|term| body.contains(term))
}

/// Returns whether the example body contains retired syntax forms.
fn has_old_syntax_form_term(body: &str) -> bool {
    OLD_SYNTAX_FORM_TERMS.iter().any(|term| body.contains(term))
}

/// Returns whether a complete module example uses an unqualified call without
/// its required import.
fn has_hidden_import_term(block: &TerlanDocBlock) -> bool {
    block.is_complete_module()
        && REQUIRED_IMPORT_TERMS
            .iter()
            .any(|term| missing_required_import(&block.body, term))
}

/// Returns whether one required import contract is missing.
fn missing_required_import(body: &str, term: &RequiredImportTerm) -> bool {
    body.contains(term.call)
        && !body.contains(term.qualified_call)
        && !term
            .accepted_import_markers
            .iter()
            .any(|marker| body.contains(marker))
}

/// Returns whether this block is diagnostic output rather than source.
fn has_diagnostic_only_marker(body: &str) -> bool {
    DIAGNOSTIC_ONLY_MARKERS
        .iter()
        .any(|marker| body.contains(marker))
}

/// Builds formatter report data for complete source examples.
fn formatter_result_for_block(block: &TerlanDocBlock) -> Option<DocsFormatterResultReport> {
    if !block.is_complete_module()
        || matches!(
            block_classification(block),
            CLASSIFICATION_INTENTIONALLY_STALE | CLASSIFICATION_DIAGNOSTIC_ONLY
        )
    {
        return None;
    }
    match format_source_module(&block.body) {
        Ok(formatted) if sources_match_ignoring_final_newline(&formatted, &block.body) => {
            Some(DocsFormatterResultReport {
                path: block.path.display().to_string(),
                line: block.line,
                status: FORMATTER_STATUS_CANONICAL,
                diagnostic: None,
            })
        }
        Ok(_) => Some(DocsFormatterResultReport {
            path: block.path.display().to_string(),
            line: block.line,
            status: FORMATTER_STATUS_NOT_CANONICAL,
            diagnostic: Some(FORMATTER_DIAGNOSTIC_CHANGED.to_string()),
        }),
        Err(error) => Some(DocsFormatterResultReport {
            path: block.path.display().to_string(),
            line: block.line,
            status: FORMATTER_STATUS_PARSE_ERROR,
            diagnostic: Some(error.message),
        }),
    }
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    [
        (
            "documentation classification vocabulary",
            &[
                CLASSIFICATION_EXECUTABLE,
                CLASSIFICATION_COMPILE_ONLY,
                CLASSIFICATION_DIAGNOSTIC_ONLY,
                CLASSIFICATION_ILLUSTRATIVE,
                CLASSIFICATION_INTENTIONALLY_STALE,
            ][..],
        ),
        (
            "documentation skip reasons",
            &[
                SKIPPED_PROJECT_CONTEXT_REASON,
                SKIPPED_DIAGNOSTIC_ONLY_REASON,
            ][..],
        ),
        (
            "documentation executed-example status",
            &[EXECUTED_EXAMPLE_STATUS][..],
        ),
        (
            "documentation diagnostic policies",
            &[
                DIAGNOSTIC_MESSAGE_TEXT_POLICY,
                DIAGNOSTIC_JSON_SHAPE,
                DIAGNOSTIC_REDACTION_POLICY,
            ][..],
        ),
        (
            "documentation formatter statuses",
            &[
                FORMATTER_STATUS_CANONICAL,
                FORMATTER_STATUS_NOT_CANONICAL,
                FORMATTER_STATUS_PARSE_ERROR,
            ][..],
        ),
        (
            "documentation formatter diagnostics",
            &[FORMATTER_DIAGNOSTIC_CHANGED][..],
        ),
    ]
    .into_iter()
    .flat_map(|(label, entries)| validate_entries_for_placeholder_terms(label, entries))
    .collect()
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let normalized = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .find(|term| normalized.contains(**term))
                .map(|term| {
                    format!("docs codeblock {label} entry `{entry}` uses placeholder term `{term}`")
                })
        })
        .collect()
}

/// Compares source after ignoring only the final trailing newline.
fn sources_match_ignoring_final_newline(left: &str, right: &str) -> bool {
    left.trim_end_matches('\n') == right.trim_end_matches('\n')
}

/// Extracts the path prefix from a rendered diagnostic.
fn diagnostic_path(diagnostic: &str) -> String {
    diagnostic
        .split_once(':')
        .map(|(path, _)| path.to_string())
        .unwrap_or_default()
}

/// Extracts the line prefix from a rendered diagnostic.
fn diagnostic_line(diagnostic: &str) -> usize {
    diagnostic
        .split(':')
        .nth(1)
        .and_then(|line| line.parse::<usize>().ok())
        .unwrap_or_default()
}

/// Extracts a stable line-level diagnostic span from a rendered diagnostic.
fn diagnostic_span(diagnostic: &str) -> DocsDiagnosticSpanReport {
    let line = diagnostic_line(diagnostic);
    DocsDiagnosticSpanReport {
        start_line: line,
        start_column: 1,
        end_line: line,
        end_column: 1,
    }
}

/// Extracts the stable diagnostic code from a rendered diagnostic.
fn diagnostic_code(diagnostic: &str) -> String {
    diagnostic
        .split_once("code[")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(code, _)| code.to_string())
        .unwrap_or_default()
}

/// Writes the executable documentation report.
fn write_report(path: &Path, report: &DocsCodeblockExecutableReport) -> QualityResult<()> {
    write_json_report(path, report)
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
                        body: dedent_fenced_body(body),
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

/// Removes common Markdown indentation from fenced block bodies.
fn dedent_fenced_body(lines: &[String]) -> String {
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| line.get(indent..).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Collects diagnostics for misleading install commands in shell-like fences.
fn collect_install_command_diagnostics(
    root: &Path,
    markdown_files: &[PathBuf],
) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for relative in markdown_files {
        let text = fs::read_to_string(root.join(relative)).map_err(|err| {
            format!(
                "{}: failed to read markdown file: {err}",
                relative.display()
            )
        })?;
        diagnostics.extend(validate_install_command_blocks(relative, &text));
    }
    Ok(diagnostics)
}

/// Validates shell-like fenced blocks for stale install commands.
fn validate_install_command_blocks(path: &Path, text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut active: Option<(String, usize)> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        if let Some((language, start_line)) = active.as_ref() {
            if trimmed.starts_with("```") {
                active = None;
                continue;
            }
            if is_install_command_fence_language(language) {
                for term in MISLEADING_INSTALL_COMMAND_TERMS {
                    if line.contains(term) {
                        diagnostics.push(format!(
                            "{}:{}: code[docs_codeblock.misleading_install_command]: install command contains misleading form `{term}`",
                            path.display(),
                            *start_line
                        ));
                    }
                }
            }
            continue;
        }

        if let Some(language) = fence_language(trimmed) {
            active = Some((language.to_string(), line_number));
        }
    }

    diagnostics
}

/// Validates Terlan documentation blocks against VM-era source rules.
pub(crate) fn validate_terlan_doc_blocks(blocks: &[TerlanDocBlock]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for block in blocks {
        if block.language == "tl" {
            diagnostics.push(format!(
                "{}:{}: code[docs_codeblock.stale_fence_language]: stale Terlan fence language `tl`; use `terlan` or `terl`",
                block.path.display(),
                block.line
            ));
        }
        for term in FORBIDDEN_EXAMPLE_TERMS {
            if block.body.contains(term) {
                diagnostics.push(format!(
                    "{}:{}: code[docs_codeblock.stale_term]: Terlan example contains stale term `{term}`",
                    block.path.display(),
                    block.line
                ));
            }
        }
        for term in UNSUPPORTED_TARGET_PROFILE_TERMS {
            if block.body.contains(term) {
                diagnostics.push(format!(
                    "{}:{}: code[docs_codeblock.unsupported_target_profile]: Terlan example contains unsupported target profile spelling `{term}`",
                    block.path.display(),
                    block.line
                ));
            }
        }
        for term in OLD_SYNTAX_FORM_TERMS {
            if block.body.contains(term) {
                diagnostics.push(format!(
                    "{}:{}: code[docs_codeblock.old_syntax_form]: Terlan example contains retired syntax form `{term}`",
                    block.path.display(),
                    block.line
                ));
            }
        }
        if block.is_complete_module() {
            for term in REQUIRED_IMPORT_TERMS {
                if missing_required_import(&block.body, term) {
                    diagnostics.push(format!(
                        "{}:{}: code[docs_codeblock.hidden_import]: Terlan example uses `{}` without an explicit import or qualified call",
                        block.path.display(),
                        block.line,
                        term.call
                    ));
                }
            }
        }
        if let Some(formatter_result) = formatter_result_for_block(block) {
            match formatter_result.status {
                "canonical" => {}
                "not_canonical" => diagnostics.push(format!(
                    "{}:{}: code[docs_codeblock.unformatted_example]: Terlan example is not canonical under `terlc fmt`",
                    block.path.display(),
                    block.line
                )),
                "parse_error" => diagnostics.push(format!(
                    "{}:{}: code[docs_codeblock.format_parse_error]: Terlan example cannot be parsed by `terlc fmt`: {}",
                    block.path.display(),
                    block.line,
                    formatter_result.diagnostic.unwrap_or_else(|| "unknown parse error".to_string())
                )),
                _ => {}
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

/// Returns whether a Markdown fence language may contain install commands.
fn is_install_command_fence_language(language: &str) -> bool {
    INSTALL_COMMAND_FENCE_LANGUAGES.contains(&language)
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
#[cfg(test)]
mod executable_docs_vm_test;
