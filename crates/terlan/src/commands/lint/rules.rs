use std::fs;
use std::path::{Path, PathBuf};

use super::diagnostic::{LintDiagnostic, Severity};
use actor_vm::actor_vm_diagnostics;
use complexity::{file_size_diagnostics, function_size_diagnostics, match_arm_size_diagnostics};
use consistency::{
    declaration_order_diagnostics, import_order_diagnostics, module_order_diagnostic,
    single_module_diagnostics, std_module_path_diagnostics,
};
use generated::{
    generated_lint_suppression_diagnostics, generated_skip_manifest_diagnostics,
    generated_source_manifest_diagnostics,
};
use imports::{
    default_import_could_be_selected_diagnostics, duplicate_import_diagnostics,
    duplicate_selected_import_diagnostics, grouped_selected_import_diagnostics,
    import_sort_diagnostics, redundant_selected_qualifier_diagnostics,
    selected_import_sort_diagnostics, unused_module_import_diagnostics,
    unused_selected_import_diagnostics,
};
use maintainability::debug_call_diagnostics;
use naming::{
    binding_snake_case_diagnostics, case_underscore_collision_diagnostics,
    function_snake_case_diagnostics, type_upper_camel_diagnostics,
};
use pipe::{fix_pipe_candidates, pipe_candidate_diagnostics};
use readability::{
    boolean_heavy_branch_diagnostics, callback_name_diagnostics, deep_expression_diagnostics,
    doc_comment_spacing_diagnostics, public_docs_diagnostics, redundant_comment_diagnostics,
    unused_destructure_binding_diagnostics,
};
use targets::incompatible_std_target_import_diagnostics;
use test_rules::fake_test_diagnostics;

mod actor_vm;
mod complexity;
mod consistency;
mod generated;
mod imports;
mod maintainability;
mod naming;
mod pipe;
mod readability;
mod targets;
mod test_rules;

const SEMICOLON_CHAIN_RULE_ID: &str = "TL0001";
const SEMICOLON_CHAIN_RULE_NAME: &str = "readability.semicolon-chain";

/// Applies only safe source-text fixes.
pub(super) fn apply_safe_fixes(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let source = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let fixed = fix_pipe_candidates(&fix_semicolon_chains(&source));
        if fixed != source {
            fs::write(path, fixed)
                .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

/// Finds lint diagnostics in one source file.
pub(super) fn lint_source(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = source
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| semicolon_chain_diagnostic(path, line_index, line))
        .collect::<Vec<_>>();
    diagnostics.extend(function_size_diagnostics(path, source));
    diagnostics.extend(match_arm_size_diagnostics(path, source));
    diagnostics.extend(file_size_diagnostics(path, source));
    diagnostics.extend(actor_vm_diagnostics(path, source));
    diagnostics.extend(debug_call_diagnostics(path, source));
    if let Some(diagnostic) = module_order_diagnostic(path, source) {
        diagnostics.push(diagnostic);
    }
    diagnostics.extend(import_order_diagnostics(path, source));
    diagnostics.extend(single_module_diagnostics(path, source));
    diagnostics.extend(declaration_order_diagnostics(path, source));
    diagnostics.extend(std_module_path_diagnostics(path, source));
    diagnostics.extend(deep_expression_diagnostics(path, source));
    diagnostics.extend(callback_name_diagnostics(path, source));
    diagnostics.extend(unused_destructure_binding_diagnostics(path, source));
    diagnostics.extend(redundant_comment_diagnostics(path, source));
    diagnostics.extend(public_docs_diagnostics(path, source));
    diagnostics.extend(doc_comment_spacing_diagnostics(path, source));
    diagnostics.extend(boolean_heavy_branch_diagnostics(path, source));
    diagnostics.extend(pipe_candidate_diagnostics(path, source));
    diagnostics.extend(fake_test_diagnostics(path, source));
    diagnostics.extend(duplicate_import_diagnostics(path, source));
    diagnostics.extend(duplicate_selected_import_diagnostics(path, source));
    diagnostics.extend(import_sort_diagnostics(path, source));
    diagnostics.extend(selected_import_sort_diagnostics(path, source));
    diagnostics.extend(grouped_selected_import_diagnostics(path, source));
    diagnostics.extend(unused_selected_import_diagnostics(path, source));
    diagnostics.extend(unused_module_import_diagnostics(path, source));
    diagnostics.extend(redundant_selected_qualifier_diagnostics(path, source));
    diagnostics.extend(default_import_could_be_selected_diagnostics(path, source));
    diagnostics.extend(function_snake_case_diagnostics(path, source));
    diagnostics.extend(type_upper_camel_diagnostics(path, source));
    diagnostics.extend(case_underscore_collision_diagnostics(path, source));
    diagnostics.extend(binding_snake_case_diagnostics(path, source));
    diagnostics.extend(generated_source_manifest_diagnostics(path, source));
    diagnostics.extend(generated_lint_suppression_diagnostics(path, source));
    diagnostics.extend(generated_skip_manifest_diagnostics(path, source));
    diagnostics.extend(incompatible_std_target_import_diagnostics(path, source));
    diagnostics
}

/// Builds a diagnostic for one dense semicolon chain line.
fn semicolon_chain_diagnostic(
    path: &Path,
    line_index: usize,
    line: &str,
) -> Option<LintDiagnostic> {
    let column = line.find(';')? + 1;
    if !has_semicolon_chain(line) {
        return None;
    }

    Some(LintDiagnostic {
        path: path.to_path_buf(),
        line: line_index + 1,
        column,
        rule_id: SEMICOLON_CHAIN_RULE_ID,
        rule_name: SEMICOLON_CHAIN_RULE_NAME,
        severity: Severity::Warning,
        message: "split dense semicolon expression chains into one expression per line",
        fix_available: can_fix_semicolon_chain(line),
    })
}

/// Returns whether a line contains a dense same-line semicolon chain.
fn has_semicolon_chain(line: &str) -> bool {
    if line.trim_start().starts_with("//") || line.trim_start().starts_with('*') {
        return false;
    }
    semicolon_split_parts(line)
        .is_some_and(|parts| parts.len() > 1 && parts.iter().all(|part| !part.trim().is_empty()))
}

/// Returns whether a semicolon chain can be rewritten by source text alone.
fn can_fix_semicolon_chain(line: &str) -> bool {
    semicolon_split_parts(line)
        .is_some_and(|parts| parts.len() > 1 && parts.iter().all(|part| !part.trim().is_empty()))
}

/// Rewrites simple same-line semicolon chains into one expression per line.
pub(super) fn fix_semicolon_chains(source: &str) -> String {
    let mut output = String::new();
    for segment in source.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        if can_fix_semicolon_chain(line) {
            output.push_str(&split_semicolon_chain_line(line));
            output.push_str(newline);
        } else {
            output.push_str(segment);
        }
    }
    output
}

/// Splits one safe semicolon-chain line while preserving indentation.
fn split_semicolon_chain_line(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let parts = semicolon_split_parts(line)
        .unwrap_or_else(|| vec![line])
        .into_iter()
        .filter_map(|part| {
            let trimmed = part.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .collect::<Vec<_>>();
    let last_index = parts.len().saturating_sub(1);

    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index == last_index {
                format!("{indent}{part}")
            } else {
                format!("{indent}{part};")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Splits a line by semicolons that are outside strings and comments.
fn semicolon_split_parts(line: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut saw_semicolon = false;

    let mut chars = line.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            continue;
        }

        if ch == '/' && chars.peek().is_some_and(|(_, next)| *next == '/') {
            return None;
        }

        if ch == ';' {
            saw_semicolon = true;
            parts.push(&line[start..index]);
            start = index + ch.len_utf8();
        }
    }

    if !saw_semicolon || in_string {
        return None;
    }

    parts.push(&line[start..]);
    Some(parts)
}
