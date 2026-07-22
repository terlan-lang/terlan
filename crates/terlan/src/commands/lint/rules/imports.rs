use std::collections::BTreeMap;
use std::path::Path;

use super::super::diagnostic::{LintDiagnostic, Severity};

mod redundant;
mod selected_default;

const DUPLICATE_IMPORT_RULE_ID: &str = "TL0601";
const DUPLICATE_IMPORT_RULE_NAME: &str = "imports.duplicate";
const DUPLICATE_SELECTED_IMPORT_RULE_ID: &str = "TL0602";
const DUPLICATE_SELECTED_IMPORT_RULE_NAME: &str = "imports.duplicate-selected";
const IMPORT_SORT_RULE_ID: &str = "TL0603";
const IMPORT_SORT_RULE_NAME: &str = "imports.sort-order";
const SELECTED_IMPORT_SORT_RULE_ID: &str = "TL0604";
const SELECTED_IMPORT_SORT_RULE_NAME: &str = "imports.selected-sort-order";
const GROUPED_SELECTED_IMPORT_RULE_ID: &str = "TL0605";
const GROUPED_SELECTED_IMPORT_RULE_NAME: &str = "imports.grouped-selected";
const UNUSED_SELECTED_IMPORT_RULE_ID: &str = "TL0606";
const UNUSED_SELECTED_IMPORT_RULE_NAME: &str = "imports.unused-selected";
const UNUSED_MODULE_IMPORT_RULE_ID: &str = "TL0607";
const UNUSED_MODULE_IMPORT_RULE_NAME: &str = "imports.unused-module";

/// Builds diagnostics for exact duplicate import declarations.
pub(super) fn duplicate_import_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut first_import_lines = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let import_key = trimmed.to_string();
        if first_import_lines
            .insert(import_key, line_index + 1)
            .is_some()
        {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: DUPLICATE_IMPORT_RULE_ID,
                rule_name: DUPLICATE_IMPORT_RULE_NAME,
                severity: Severity::Warning,
                message: "duplicate import declaration; keep one import",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Builds diagnostics for duplicate selected names in one import declaration.
pub(super) fn duplicate_selected_import_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let Some(selected_names) = selected_import_names(trimmed) else {
            continue;
        };
        let mut seen_names = BTreeMap::new();
        for name in selected_names {
            if seen_names.insert(name.to_string(), ()).is_some() {
                diagnostics.push(LintDiagnostic {
                    path: path.to_path_buf(),
                    line: line_index + 1,
                    column: line.len() - line.trim_start().len() + 1,
                    rule_id: DUPLICATE_SELECTED_IMPORT_RULE_ID,
                    rule_name: DUPLICATE_SELECTED_IMPORT_RULE_NAME,
                    severity: Severity::Warning,
                    message: "duplicate selected import name; keep one selected name",
                    fix_available: false,
                });
                break;
            }
        }
    }

    diagnostics
}

/// Builds diagnostics for import declarations that are not sorted.
pub(super) fn import_sort_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut previous_import: Option<&str> = None;

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        if previous_import.is_some_and(|previous| trimmed < previous) {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: IMPORT_SORT_RULE_ID,
                rule_name: IMPORT_SORT_RULE_NAME,
                severity: Severity::Warning,
                message: "import declarations should be sorted for stable module structure",
                fix_available: false,
            });
        }
        previous_import = Some(trimmed);
    }

    diagnostics
}

/// Builds diagnostics for selected import names that are not sorted.
pub(super) fn selected_import_sort_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let Some(selected_names) = selected_import_names(trimmed) else {
            continue;
        };
        if selected_names.windows(2).any(|names| names[1] < names[0]) {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: SELECTED_IMPORT_SORT_RULE_ID,
                rule_name: SELECTED_IMPORT_SORT_RULE_NAME,
                severity: Severity::Warning,
                message: "selected import names should be sorted for stable import lists",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Builds diagnostics for split selected imports from the same module.
pub(super) fn grouped_selected_import_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let mut seen_modules = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let Some((is_type, module_name)) = selected_import_module(trimmed) else {
            continue;
        };
        let key = (is_type, module_name.to_string());
        if seen_modules.insert(key, line_index + 1).is_some() {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: GROUPED_SELECTED_IMPORT_RULE_ID,
                rule_name: GROUPED_SELECTED_IMPORT_RULE_NAME,
                severity: Severity::Warning,
                message: "selected imports from the same module should be grouped",
                fix_available: false,
            });
        }
    }

    diagnostics
}

/// Builds diagnostics for selected import names absent from source use sites.
pub(super) fn unused_selected_import_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let searchable_source = source_without_imports_and_comments(source);
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let Some(selected_names) = selected_import_names(trimmed) else {
            continue;
        };
        for name in selected_names {
            let visible_name = selected_import_visible_name(name);
            if is_upper_camel_import_name(visible_name) {
                continue;
            }
            if !contains_identifier(&searchable_source, visible_name) {
                diagnostics.push(LintDiagnostic {
                    path: path.to_path_buf(),
                    line: line_index + 1,
                    column: line.len() - line.trim_start().len() + 1,
                    rule_id: UNUSED_SELECTED_IMPORT_RULE_ID,
                    rule_name: UNUSED_SELECTED_IMPORT_RULE_NAME,
                    severity: Severity::Warning,
                    message: "selected import name is unused; remove it or use the import",
                    fix_available: false,
                });
            }
        }
    }

    diagnostics
}

/// Builds diagnostics for ordinary module imports absent from source use sites.
pub(super) fn unused_module_import_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let searchable_source = source_without_imports_and_comments(source);
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let trimmed = line.trim();
        let Some(import) = ordinary_module_import(trimmed) else {
            continue;
        };
        if contains_identifier(&searchable_source, import.visible_name)
            || searchable_source.contains(import.module_name)
        {
            continue;
        }
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line: line_index + 1,
            column: line.len() - line.trim_start().len() + 1,
            rule_id: UNUSED_MODULE_IMPORT_RULE_ID,
            rule_name: UNUSED_MODULE_IMPORT_RULE_NAME,
            severity: Severity::Warning,
            message: "module import is unused; remove it or use the imported module",
            fix_available: false,
        });
    }

    diagnostics
}

/// Builds diagnostics for whole-module imports that could be selected imports.
pub(super) fn default_import_could_be_selected_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    selected_default::default_import_could_be_selected_diagnostics(path, source)
}

/// Builds diagnostics for qualified calls made redundant by selected imports.
pub(super) fn redundant_selected_qualifier_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    redundant::redundant_selected_qualifier_diagnostics(path, source)
}

/// One ordinary module import with the module path and source-visible name.
pub(super) struct OrdinaryModuleImport<'a> {
    pub(super) module_name: &'a str,
    pub(super) visible_name: &'a str,
    pub(super) is_aliased: bool,
}

/// Returns ordinary module import data for `import module.Name.` declarations.
pub(super) fn ordinary_module_import(import_line: &str) -> Option<OrdinaryModuleImport<'_>> {
    let rest = import_line.strip_prefix("import ")?;
    if rest.starts_with("type ")
        || rest.starts_with("css ")
        || rest.starts_with("file ")
        || rest.contains('{')
    {
        return None;
    }

    let (module_part, alias_part) = rest
        .split_once(" as ")
        .map_or((rest, None), |(module, alias)| (module, Some(alias)));
    let module_name = module_part.trim().trim_end_matches('.');
    if module_name.is_empty() || module_name.starts_with('"') {
        return None;
    }
    let visible_name = alias_part
        .map(|alias| alias.trim().trim_end_matches('.'))
        .filter(|alias| !alias.is_empty())
        .unwrap_or_else(|| module_name.rsplit('.').next().unwrap_or(module_name));

    Some(OrdinaryModuleImport {
        module_name,
        visible_name,
        is_aliased: alias_part.is_some(),
    })
}

/// Returns selected import module identity as `(is_type, module_name)`.
fn selected_import_module(import_line: &str) -> Option<(bool, &str)> {
    let rest = import_line.strip_prefix("import ")?;
    let (is_type, rest) = rest
        .strip_prefix("type ")
        .map_or((false, rest), |typed| (true, typed));
    let selected_start = rest.find('{')?;
    let module_name = rest[..selected_start].trim_end_matches('.');
    if module_name.is_empty() {
        return None;
    }
    Some((is_type, module_name))
}

/// Returns the source-visible name for a selected import item.
fn selected_import_visible_name(import_name: &str) -> &str {
    import_name
        .split_once(" as ")
        .map_or(import_name, |(_, alias)| alias)
        .trim()
}

/// Returns whether an import item is a type or constructor-shaped name.
fn is_upper_camel_import_name(import_name: &str) -> bool {
    import_name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Returns selected import names from `import module.{a, b}.` source.
fn selected_import_names(import_line: &str) -> Option<Vec<&str>> {
    let selected_start = import_line.find('{')?;
    let selected_end = import_line[selected_start + 1..].find('}')? + selected_start + 1;
    if selected_end <= selected_start + 1 {
        return None;
    }
    let selected = &import_line[selected_start + 1..selected_end];
    Some(
        selected
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect(),
    )
}

/// Returns source text with import declarations and comments removed.
fn source_without_imports_and_comments(source: &str) -> String {
    let mut in_block_comment = false;
    let mut output = String::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || trimmed.starts_with("//") {
            output.push('\n');
            continue;
        }
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            output.push('\n');
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            output.push('\n');
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }

    output
}

/// Returns non-import source lines while skipping comments and blank lines.
fn non_import_source_lines(source: &str) -> Vec<(usize, &str)> {
    let mut in_block_comment = false;
    let mut lines = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("import ") || trimmed.starts_with("//") {
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
        lines.push((line_index, line));
    }

    lines
}

/// Returns whether source text contains `name` as a standalone identifier.
fn contains_identifier(source: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    source.match_indices(name).any(|(index, matched)| {
        let before = source[..index].chars().next_back();
        let after = source[index + matched.len()..].chars().next();
        !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
    })
}

/// Returns whether a character is part of a Terlan identifier.
pub(super) fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

/// One real import declaration found outside comments.
pub(super) struct ImportLine<'a> {
    pub(super) line_index: usize,
    pub(super) line: &'a str,
}

/// Returns real import lines while skipping line and block comments.
pub(super) fn import_lines(source: &str) -> Vec<ImportLine<'_>> {
    let mut in_block_comment = false;
    let mut imports = Vec::new();

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
        if trimmed.starts_with("import ") {
            imports.push(ImportLine { line_index, line });
        }
    }

    imports
}
