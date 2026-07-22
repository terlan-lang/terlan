use std::path::Path;

use super::super::diagnostic::{LintDiagnostic, Severity};

const INCOMPATIBLE_STD_TARGET_RULE_ID: &str = "TL0702";
const INCOMPATIBLE_STD_TARGET_RULE_NAME: &str = "targets.incompatible-std";

/// Builds diagnostics for mixed incompatible target-specific std imports.
pub(super) fn incompatible_std_target_import_diagnostics(
    path: &Path,
    source: &str,
) -> Vec<LintDiagnostic> {
    let mut seen_families = Vec::new();
    let mut diagnostics = Vec::new();

    for ImportLine { line_index, line } in import_lines(source) {
        let Some(module_name) = import_module_name(line.trim()) else {
            continue;
        };
        let Some(family) = TargetStdFamily::from_module(module_name) else {
            continue;
        };
        if seen_families
            .iter()
            .any(|seen| family.is_incompatible_with(*seen))
        {
            diagnostics.push(LintDiagnostic {
                path: path.to_path_buf(),
                line: line_index + 1,
                column: line.len() - line.trim_start().len() + 1,
                rule_id: INCOMPATIBLE_STD_TARGET_RULE_ID,
                rule_name: INCOMPATIBLE_STD_TARGET_RULE_NAME,
                severity: Severity::Warning,
                message: "source mixes incompatible target-specific std imports",
                fix_available: false,
            });
            break;
        }
        if !seen_families.contains(&family) {
            seen_families.push(family);
        }
    }

    diagnostics
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetStdFamily {
    Js,
    Wasm,
    Vm,
    Native,
}

impl TargetStdFamily {
    fn from_module(module_name: &str) -> Option<Self> {
        if module_name == "std.js" || module_name.starts_with("std.js.") {
            Some(Self::Js)
        } else if module_name == "std.wasm" || module_name.starts_with("std.wasm.") {
            Some(Self::Wasm)
        } else if module_name == "std.vm" || module_name.starts_with("std.vm.") {
            Some(Self::Vm)
        } else if module_name == "std.native" || module_name.starts_with("std.native.") {
            Some(Self::Native)
        } else {
            None
        }
    }

    fn is_incompatible_with(self, other: Self) -> bool {
        !matches!(
            (self, other),
            (Self::Js, Self::Js)
                | (Self::Wasm, Self::Wasm)
                | (Self::Vm, Self::Vm)
                | (Self::Native, Self::Native)
                | (Self::Vm, Self::Native)
                | (Self::Native, Self::Vm)
        )
    }
}

fn import_module_name(import_line: &str) -> Option<&str> {
    let rest = import_line.strip_prefix("import ")?;
    let rest = rest.strip_prefix("type ").unwrap_or(rest);
    let end = rest
        .find('{')
        .or_else(|| rest.find(" as "))
        .unwrap_or(rest.len());
    let module_name = rest[..end].trim().trim_end_matches('.');
    if module_name.is_empty() {
        None
    } else {
        Some(module_name)
    }
}

struct ImportLine<'a> {
    line_index: usize,
    line: &'a str,
}

fn import_lines(source: &str) -> Vec<ImportLine<'_>> {
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
