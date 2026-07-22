use std::path::Path;

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};

use super::{
    import_lines, is_identifier_char, is_upper_camel_import_name, non_import_source_lines,
    selected_import_module, selected_import_names,
};

const REDUNDANT_SELECTED_QUALIFIER_RULE_ID: &str = "TL0608";
const REDUNDANT_SELECTED_QUALIFIER_RULE_NAME: &str = "imports.redundant-qualifier";

/// Builds diagnostics for qualified calls made redundant by selected imports.
pub(super) fn redundant_selected_qualifier_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let imports = selected_value_imports(source);
    if imports.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for (line_index, line) in non_import_source_lines(source) {
        for import in &imports {
            if let Some(column) = redundant_qualified_call_column(line, import) {
                diagnostics.push(LintDiagnostic {
                    path: path.to_path_buf(),
                    line: line_index + 1,
                    column,
                    rule_id: REDUNDANT_SELECTED_QUALIFIER_RULE_ID,
                    rule_name: REDUNDANT_SELECTED_QUALIFIER_RULE_NAME,
                    severity: Severity::Suggestion,
                    message: "selected import already makes this call unambiguous; use the selected name",
                    fix_available: false,
                });
                break;
            }
        }
    }

    diagnostics
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedValueImport {
    module_name: String,
    module_visible_name: String,
    item_name: String,
}

fn selected_value_imports(source: &str) -> Vec<SelectedValueImport> {
    let mut imports = Vec::new();
    for import_line in import_lines(source) {
        let trimmed = import_line.line.trim();
        let Some((is_type, module_name)) = selected_import_module(trimmed) else {
            continue;
        };
        if is_type {
            continue;
        }
        let Some(selected_names) = selected_import_names(trimmed) else {
            continue;
        };
        let module_visible_name = module_name
            .rsplit('.')
            .next()
            .unwrap_or(module_name)
            .to_string();
        for selected_name in selected_names {
            let item_name = selected_import_source_name(selected_name);
            if is_upper_camel_import_name(item_name) {
                continue;
            }
            imports.push(SelectedValueImport {
                module_name: module_name.to_string(),
                module_visible_name: module_visible_name.clone(),
                item_name: item_name.to_string(),
            });
        }
    }
    imports
}

fn selected_import_source_name(import_name: &str) -> &str {
    import_name
        .split_once(" as ")
        .map_or(import_name, |(name, _)| name)
        .trim()
}

fn redundant_qualified_call_column(line: &str, import: &SelectedValueImport) -> Option<usize> {
    let full_call = format!("{}.{}(", import.module_name, import.item_name);
    let visible_call = format!("{}.{}(", import.module_visible_name, import.item_name);
    first_code_match_column(line, &full_call)
        .or_else(|| first_code_match_column(line, &visible_call))
}

fn first_code_match_column(line: &str, needle: &str) -> Option<usize> {
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
        if line[index..].starts_with(needle) && has_call_boundaries(line, index, needle.len()) {
            return Some(index + 1);
        }
        index += 1;
    }

    None
}

fn has_call_boundaries(line: &str, start: usize, len: usize) -> bool {
    let before = line[..start].chars().next_back();
    let after = line[start + len..].chars().next();
    !before.is_some_and(|ch| is_identifier_char(ch) || ch == '.')
        && !after.is_some_and(is_identifier_char)
}
