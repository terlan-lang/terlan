use std::path::Path;

use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload};

use super::super::diagnostic::{LintDiagnostic, Severity};

const FUNCTION_SIZE_RULE_ID: &str = "TL0901";
const FUNCTION_SIZE_RULE_NAME: &str = "complexity.function-size";
const FILE_SIZE_RULE_ID: &str = "TL0902";
const FILE_SIZE_RULE_NAME: &str = "complexity.file-size";
const MATCH_ARM_SIZE_RULE_ID: &str = "TL0903";
const MATCH_ARM_SIZE_RULE_NAME: &str = "complexity.match-arm-size";
const MAX_FUNCTION_CLAUSE_LINES: usize = 40;
const MAX_ORDINARY_SOURCE_LINES: usize = 500;
const MAX_MATCH_ARM_LINES: usize = 20;

/// Builds diagnostics for function or method clauses that exceed size limits.
pub(super) fn function_size_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Ok(module) = parse_module_as_syntax_output(source) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    for declaration in module.declarations {
        let clauses = match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => clauses,
            _ => continue,
        };
        for clause in clauses {
            if span_line_count(source, clause.span.start, clause.span.end)
                <= MAX_FUNCTION_CLAUSE_LINES
            {
                continue;
            }
            let (line, column) = line_column_at_offset(source, clause.span.start);
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line,
                column,
                rule_id: FUNCTION_SIZE_RULE_ID,
                rule_name: FUNCTION_SIZE_RULE_NAME,
                severity: Severity::Warning,
                message: "function clause exceeds the maintainability line threshold",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Builds diagnostics for ordinary Terlan source files that exceed size limits.
pub(super) fn file_size_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if is_generated_source(source) {
        return Vec::new();
    }
    let line_count = source.lines().count();
    if line_count <= MAX_ORDINARY_SOURCE_LINES {
        return Vec::new();
    }

    vec![LintDiagnostic {
        path: path.to_path_buf(),
        line: 1,
        column: 1,
        rule_id: FILE_SIZE_RULE_ID,
        rule_name: FILE_SIZE_RULE_NAME,
        severity: Severity::Warning,
        message: "ordinary source file exceeds the maintainability line threshold",
        fix_available: false,
    }]
}

/// Builds diagnostics for oversized case or if branch arms.
pub(super) fn match_arm_size_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut contexts = Vec::<BranchContext>::new();
    let mut depth = 0;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let active_index = contexts.len().checked_sub(1);
        let is_arm = active_index.is_some_and(|index| {
            depth == contexts[index].branch_depth && branch_arrow_column(line).is_some()
        });

        if let Some(index) = active_index {
            if is_arm {
                finish_active_arm(path, &mut diagnostics, &mut contexts[index]);
                start_active_arm(&mut contexts[index], line_index, line);
            } else if contexts[index].active_arm.is_some() && is_counted_arm_body_line(trimmed) {
                contexts[index]
                    .active_arm
                    .as_mut()
                    .expect("active arm")
                    .line_count += 1;
            }
        }

        let depth_before = depth;
        let depth_after = update_depth_for_line(depth_before, line);
        if opens_branch_expression(trimmed) && depth_after > depth_before {
            contexts.push(BranchContext {
                branch_depth: depth_after,
                active_arm: None,
            });
        }

        while contexts
            .last()
            .is_some_and(|context| depth_after < context.branch_depth)
        {
            if let Some(mut context) = contexts.pop() {
                finish_active_arm(path, &mut diagnostics, &mut context);
            }
        }
        depth = depth_after;
    }

    for mut context in contexts.into_iter().rev() {
        finish_active_arm(path, &mut diagnostics, &mut context);
    }

    diagnostics
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BranchContext {
    branch_depth: usize,
    active_arm: Option<BranchArm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BranchArm {
    line: usize,
    column: usize,
    line_count: usize,
}

fn start_active_arm(context: &mut BranchContext, line_index: usize, line: &str) {
    let Some(column) = branch_arrow_column(line) else {
        return;
    };
    context.active_arm = Some(BranchArm {
        line: line_index + 1,
        column,
        line_count: usize::from(branch_arrow_has_inline_body(line)),
    });
}

fn finish_active_arm(
    path: &Path,
    diagnostics: &mut Vec<LintDiagnostic>,
    context: &mut BranchContext,
) {
    let Some(arm) = context.active_arm.take() else {
        return;
    };
    if arm.line_count <= MAX_MATCH_ARM_LINES {
        return;
    }
    diagnostics.push(LintDiagnostic {
        path: path.to_path_buf(),
        line: arm.line,
        column: arm.column,
        rule_id: MATCH_ARM_SIZE_RULE_ID,
        rule_name: MATCH_ARM_SIZE_RULE_NAME,
        severity: Severity::Warning,
        message: "case or if arm exceeds the maintainability line threshold",
        fix_available: false,
    });
}

fn is_generated_source(source: &str) -> bool {
    source.lines().take(20).any(|line| {
        let lowered = line.to_ascii_lowercase();
        lowered.contains("@generated")
            || lowered.contains("generated by")
            || lowered.contains("do not edit")
            || lowered.contains("this file is generated")
    })
}

fn span_line_count(source: &str, start: usize, end: usize) -> usize {
    let start = start.min(source.len());
    let end = end.min(source.len()).max(start);
    source[start..end].lines().count().max(1)
}

fn line_column_at_offset(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn opens_branch_expression(trimmed: &str) -> bool {
    !is_comment_line(trimmed)
        && trimmed.contains('{')
        && (trimmed.starts_with("if {") || trimmed.contains("case "))
}

fn branch_arrow_column(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if is_comment_line(trimmed) {
        return None;
    }
    line.find("->").map(|column| column + 1)
}

fn branch_arrow_has_inline_body(line: &str) -> bool {
    line.split_once("->")
        .is_some_and(|(_, body)| !body.trim().is_empty())
}

fn is_counted_arm_body_line(trimmed: &str) -> bool {
    !trimmed.is_empty() && !is_comment_line(trimmed) && trimmed != "}" && trimmed != "};"
}

fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//") || trimmed.starts_with('*')
}

fn update_depth_for_line(depth: usize, line: &str) -> usize {
    let mut depth = depth;
    for ch in line.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}
