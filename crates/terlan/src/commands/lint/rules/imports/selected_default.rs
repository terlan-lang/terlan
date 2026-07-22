use std::collections::BTreeMap;
use std::path::Path;

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};

use super::{
    import_lines, is_identifier_char, non_import_source_lines, ordinary_module_import,
    OrdinaryModuleImport,
};

const DEFAULT_COULD_BE_SELECTED_RULE_ID: &str = "TL0609";
const DEFAULT_COULD_BE_SELECTED_RULE_NAME: &str = "imports.default-could-be-selected";

/// Builds diagnostics for whole-module imports that could be selected imports.
pub(super) fn default_import_could_be_selected_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let code_lines = non_import_source_lines(source);
    let mut diagnostics = Vec::new();

    for import_line in import_lines(source) {
        let trimmed = import_line.line.trim();
        let Some(import) = ordinary_module_import(trimmed) else {
            continue;
        };
        if import.is_aliased || has_visible_constructor_call(&code_lines, import.visible_name) {
            continue;
        }
        let members = qualified_call_members(&code_lines, &import);
        if members.len() == 1 {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: import_line.line_index + 1,
                column: import_line.line.len() - import_line.line.trim_start().len() + 1,
                rule_id: DEFAULT_COULD_BE_SELECTED_RULE_ID,
                rule_name: DEFAULT_COULD_BE_SELECTED_RULE_NAME,
                severity: Severity::Suggestion,
                message: "module import is used for one member; prefer a selected import",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Returns whether source calls the imported module-visible constructor.
fn has_visible_constructor_call(code_lines: &[(usize, &str)], visible_name: &str) -> bool {
    let constructor_call = format!("{visible_name}(");
    code_lines
        .iter()
        .any(|(_, line)| has_constructor_call(line, &constructor_call))
}

/// Returns distinct lower-case qualified call members used for one import.
fn qualified_call_members(
    code_lines: &[(usize, &str)],
    import: &OrdinaryModuleImport<'_>,
) -> Vec<String> {
    let mut members = BTreeMap::new();
    let visible_prefix = format!("{}.", import.visible_name);
    let full_prefix = format!("{}.", import.module_name);

    for (_, line) in code_lines {
        collect_qualified_call_members(line, &visible_prefix, &mut members);
        if full_prefix != visible_prefix {
            collect_qualified_call_members(line, &full_prefix, &mut members);
        }
    }

    members.into_keys().collect()
}

/// Adds lower-case member names from qualified call sites on one source line.
fn collect_qualified_call_members(line: &str, prefix: &str, members: &mut BTreeMap<String, ()>) {
    let mut index = 0;
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut escaped = false;

    while index < bytes.len() {
        let ch = bytes[index] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            index += 1;
            continue;
        }
        if ch == '/' && bytes.get(index + 1).is_some_and(|next| *next == b'/') {
            break;
        }
        if line[index..].starts_with(prefix) {
            if let Some((member, end_index)) = qualified_call_member_at(line, index, prefix) {
                if member
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_lowercase())
                {
                    members.insert(member.to_string(), ());
                }
                index = end_index;
                continue;
            }
        }
        index += 1;
    }
}

/// Returns the member name when `prefix.member(` appears at `start`.
fn qualified_call_member_at<'a>(
    line: &'a str,
    start: usize,
    prefix: &str,
) -> Option<(&'a str, usize)> {
    let before = line[..start].chars().next_back();
    if before.is_some_and(|ch| is_identifier_char(ch) || ch == '.') {
        return None;
    }

    let member_start = start + prefix.len();
    let mut member_end = member_start;
    for (offset, ch) in line[member_start..].char_indices() {
        if is_identifier_char(ch) {
            member_end = member_start + offset + ch.len_utf8();
        } else {
            break;
        }
    }
    if member_end == member_start || line[member_end..].chars().next() != Some('(') {
        return None;
    }

    Some((&line[member_start..member_end], member_end))
}

/// Returns whether a constructor call appears outside strings/comments.
fn has_constructor_call(line: &str, needle: &str) -> bool {
    let mut index = 0;
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut escaped = false;

    while index < bytes.len() {
        let ch = bytes[index] as char;
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            index += 1;
            continue;
        }
        if ch == '/' && bytes.get(index + 1).is_some_and(|next| *next == b'/') {
            break;
        }
        if line[index..].starts_with(needle) {
            let before = line[..index].chars().next_back();
            if !before.is_some_and(|ch| is_identifier_char(ch) || ch == '.') {
                return true;
            }
        }
        index += 1;
    }

    false
}
