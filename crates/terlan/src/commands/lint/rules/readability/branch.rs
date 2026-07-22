use std::path::Path;

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};

const BOOLEAN_HEAVY_BRANCH_RULE_ID: &str = "TL0008";
const BOOLEAN_HEAVY_BRANCH_RULE_NAME: &str = "readability.boolean-heavy-branch";
const MAX_BRANCH_BOOLEAN_OPERATORS: usize = 1;

/// Builds diagnostics for branch conditions with too many boolean operators.
pub(super) fn boolean_heavy_branch_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_block_comment = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        let Some(condition) = branch_condition(line) else {
            continue;
        };
        if boolean_operator_count(condition) > MAX_BRANCH_BOOLEAN_OPERATORS {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: BOOLEAN_HEAVY_BRANCH_RULE_ID,
                rule_name: BOOLEAN_HEAVY_BRANCH_RULE_NAME,
                severity: Severity::Warning,
                message:
                    "boolean-heavy branch condition should use case, guards, or a named predicate",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Returns the condition part before a branch arrow on one source line.
fn branch_condition(line: &str) -> Option<&str> {
    let (condition, _) = line.split_once("->")?;
    let trimmed = condition.trim();
    if trimmed.is_empty() || trimmed == "_" {
        return None;
    }
    Some(trimmed.strip_prefix("if {").unwrap_or(trimmed).trim())
}

/// Counts boolean connectives in one branch condition.
fn boolean_operator_count(condition: &str) -> usize {
    condition
        .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|word| *word == "and" || *word == "or")
        .count()
}
