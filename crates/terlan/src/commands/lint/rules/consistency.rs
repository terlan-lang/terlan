use std::collections::HashSet;
use std::path::Path;

use super::super::diagnostic::{LintDiagnostic, Severity};
use crate::terlan_syntax::SyntaxDeclarationPayload;

mod named_atom;

pub(super) use named_atom::named_atom_literal_diagnostics;

const MODULE_ORDER_RULE_ID: &str = "TL0501";
const MODULE_ORDER_RULE_NAME: &str = "consistency.module-order";
const IMPORT_ORDER_RULE_ID: &str = "TL0502";
const IMPORT_ORDER_RULE_NAME: &str = "consistency.import-order";
const SINGLE_MODULE_RULE_ID: &str = "TL0503";
const SINGLE_MODULE_RULE_NAME: &str = "consistency.single-module";
const DECLARATION_ORDER_RULE_ID: &str = "TL0504";
const DECLARATION_ORDER_RULE_NAME: &str = "consistency.declaration-order";
const STD_MODULE_PATH_RULE_ID: &str = "TL0505";
const STD_MODULE_PATH_RULE_NAME: &str = "consistency.std-module-path";
const RECEIVER_CAPABILITY_RULE_ID: &str = "TL0507";
const RECEIVER_CAPABILITY_RULE_NAME: &str = "consistency.receiver-capability-ownership";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DeclarationBlock {
    TypeLike,
    Impl,
    FunctionLike,
}

/// Reports public free functions whose first parameter is a type owned by the
/// same module.
///
/// Such operations are capabilities of an existing value and should be
/// declared as receiver methods. Constructors and target-directed factories
/// have no existing owner and therefore do not match this rule.
pub(in crate::commands::lint) fn receiver_capability_ownership_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    if is_test_source(path) {
        return Vec::new();
    }
    let module = match crate::terlan_syntax::parse_module_as_syntax_output(source) {
        Ok(module) => module,
        Err(_) => return Vec::new(),
    };
    let local_types = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();

    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let (params, is_public) = match &declaration.payload {
                SyntaxDeclarationPayload::Function {
                    params,
                    is_public,
                    is_macro: false,
                    ..
                }
                | SyntaxDeclarationPayload::ConstFunction {
                    params, is_public, ..
                } => (params, *is_public),
                _ => return None,
            };
            if !is_public {
                return None;
            }
            let first_type = params
                .first()
                .and_then(|parameter| root_type_name(&parameter.annotation.text))?;
            if !local_types.contains(first_type) {
                return None;
            }
            let (line, column) = source_line_column_at(source, declaration.span.start);
            Some(LintDiagnostic {
                path: path.to_path_buf(),
                line,
                column,
                rule_id: RECEIVER_CAPABILITY_RULE_ID,
                rule_name: RECEIVER_CAPABILITY_RULE_NAME,
                severity: Severity::Warning,
                message: "declare this public type-owned operation as a receiver method",
                fix_available: true,
            })
        })
        .collect()
}

/// Rewrites public type-owned free functions into receiver declarations.
pub(in crate::commands::lint) fn fix_receiver_capability_ownership(
    path: &Path,
    source: &str,
) -> String {
    if is_test_source(path) {
        return source.to_string();
    }
    let Ok(module) = crate::terlan_syntax::parse_module_as_syntax_output(source) else {
        return source.to_string();
    };
    let local_types = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, .. }
            | SyntaxDeclarationPayload::Struct { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut edits = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let (params, is_public) = match &declaration.payload {
                SyntaxDeclarationPayload::Function {
                    params,
                    is_public,
                    is_macro: false,
                    ..
                }
                | SyntaxDeclarationPayload::ConstFunction {
                    params, is_public, ..
                } => (params, *is_public),
                _ => return None,
            };
            let first = params.first()?;
            if !is_public
                || first.has_default
                || first.pattern_text.is_some()
                || !root_type_name(&first.annotation.text)
                    .is_some_and(|name| local_types.contains(name))
            {
                return None;
            }
            receiver_declaration_edit(
                source,
                declaration.span.start,
                first.span.start,
                first.span.end,
            )
        })
        .collect::<Vec<_>>();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut fixed = source.to_string();
    for (start, end, replacement) in edits {
        fixed.replace_range(start..end, &replacement);
    }
    fixed
}

fn receiver_declaration_edit(
    source: &str,
    declaration_start: usize,
    parameter_start: usize,
    parameter_end: usize,
) -> Option<(usize, usize, String)> {
    let line_start = source
        .get(..declaration_start)?
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let before_parameter = source.get(line_start..parameter_start)?;
    let open_relative = before_parameter.rfind('(')?;
    let open = line_start + open_relative;
    let before_open = source.get(line_start..open)?;
    let public_relative = before_open.rfind("pub ")?;
    let public_start = line_start + public_relative;
    let callable_head = source.get(public_start + 4..open)?.trim();
    let parameter = source.get(parameter_start..parameter_end)?.trim();
    let after_parameter = source.get(parameter_end..)?;
    let whitespace = after_parameter.len() - after_parameter.trim_start().len();
    let delimiter = parameter_end + whitespace;
    let has_comma = source.get(delimiter..)?.starts_with(',');
    let end = delimiter + usize::from(has_comma);
    Some((
        public_start,
        end,
        format!("pub ({parameter}) {callable_head}("),
    ))
}

fn is_test_source(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.ends_with("Test"))
}

fn root_type_name(annotation: &str) -> Option<&str> {
    let root = annotation
        .trim()
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '_' && character != '.'
        })
        .next()?;
    (!root.is_empty() && !root.contains('.')).then_some(root)
}

fn source_line_column_at(source: &str, index: usize) -> (usize, usize) {
    let prefix = &source[..index.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1);
    (line, column)
}

/// Builds a diagnostic when the module declaration is not first after docs.
pub(super) fn module_order_diagnostic(path: &Path, source: &str) -> Option<LintDiagnostic> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("terls") {
        return None;
    }

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
        if trimmed.starts_with("module ") {
            return None;
        }
        return Some(LintDiagnostic {
            path: path.to_path_buf(),
            line: line_index + 1,
            column: line.len() - line.trim_start().len() + 1,
            rule_id: MODULE_ORDER_RULE_ID,
            rule_name: MODULE_ORDER_RULE_NAME,
            severity: Severity::Error,
            message: "module declaration must be the first non-comment declaration",
            fix_available: false,
        });
    }

    None
}

/// Builds diagnostics when imports appear after real declarations.
pub(super) fn import_order_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_block_comment = false;
    let mut seen_module = false;
    let mut seen_non_import_declaration = false;

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
        if trimmed.starts_with("module ") {
            seen_module = true;
            continue;
        }
        if trimmed.starts_with("import ") {
            if seen_non_import_declaration {
                diagnostics.push(LintDiagnostic {
                    path: path.to_path_buf(),
                    line: line_index + 1,
                    column: line.len() - line.trim_start().len() + 1,
                    rule_id: IMPORT_ORDER_RULE_ID,
                    rule_name: IMPORT_ORDER_RULE_NAME,
                    severity: Severity::Error,
                    message:
                        "imports must appear before type, trait, impl, and function declarations",
                    fix_available: false,
                });
            }
            continue;
        }
        if seen_module {
            seen_non_import_declaration = true;
        }
    }

    diagnostics
}

/// Builds diagnostics when a file declares more than one module.
pub(super) fn single_module_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_block_comment = false;
    let mut seen_module = false;

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
        if !trimmed.starts_with("module ") {
            continue;
        }
        if seen_module {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: SINGLE_MODULE_RULE_ID,
                rule_name: SINGLE_MODULE_RULE_NAME,
                severity: Severity::Error,
                message: "a source file may declare exactly one module",
                fix_available: false,
            });
        }
        seen_module = true;
    }

    diagnostics
}

/// Builds diagnostics when top-level declarations are not in canonical blocks.
pub(super) fn declaration_order_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_block_comment = false;
    let mut current_block: Option<DeclarationBlock> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if line.len() != line.trim_start().len() {
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
        let Some(block) = declaration_block(trimmed) else {
            continue;
        };
        if current_block.is_some_and(|current| block < current) {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: DECLARATION_ORDER_RULE_ID,
                rule_name: DECLARATION_ORDER_RULE_NAME,
                severity: Severity::Warning,
                message:
                    "declarations should be ordered as types/shapes/traits, impls, then functions",
                fix_available: false,
            });
        }
        current_block = Some(current_block.map_or(block, |current| current.max(block)));
    }

    diagnostics
}

/// Builds a diagnostic when a std module declaration differs from its path.
pub(super) fn std_module_path_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    let Some(expected_module) = expected_std_module_path(path) else {
        return Vec::new();
    };
    let Some((line_index, line, actual_module)) = first_module_declaration(source) else {
        return Vec::new();
    };
    if actual_module == expected_module {
        return Vec::new();
    }

    vec![LintDiagnostic {
        path: path.to_path_buf(),
        line: line_index + 1,
        column: line.len() - line.trim_start().len() + 1,
        rule_id: STD_MODULE_PATH_RULE_ID,
        rule_name: STD_MODULE_PATH_RULE_NAME,
        severity: Severity::Error,
        message: "std module declaration must match the canonical source path",
        fix_available: false,
    }]
}

/// Classifies a top-level declaration line into its canonical declaration block.
fn declaration_block(trimmed_line: &str) -> Option<DeclarationBlock> {
    let line = trimmed_line.strip_prefix("pub ").unwrap_or(trimmed_line);
    if line.starts_with("type ")
        || line.starts_with("struct ")
        || line.starts_with("shape ")
        || line.starts_with("trait ")
    {
        return Some(DeclarationBlock::TypeLike);
    }
    if line.starts_with("impl ") {
        return Some(DeclarationBlock::Impl);
    }
    if line.contains("->") {
        return Some(DeclarationBlock::FunctionLike);
    }
    None
}

/// Returns the module name expected from a canonical std source path.
fn expected_std_module_path(path: &Path) -> Option<String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>();
    let std_index = components.iter().position(|component| component == "std")?;
    let mut module_parts = Vec::new();
    for component in &components[std_index..] {
        let part = component
            .strip_suffix(".terl")
            .or_else(|| component.strip_suffix(".terli"))
            .unwrap_or(component);
        if part.is_empty() {
            return None;
        }
        module_parts.push(part.to_string());
    }
    Some(module_parts.join("."))
}

/// Returns the first real module declaration outside comments.
fn first_module_declaration(source: &str) -> Option<(usize, &str, String)> {
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
        let Some(module_name) = trimmed
            .strip_prefix("module ")
            .and_then(|rest| rest.strip_suffix('.'))
        else {
            continue;
        };
        return Some((line_index, line, module_name.trim().to_string()));
    }

    None
}
