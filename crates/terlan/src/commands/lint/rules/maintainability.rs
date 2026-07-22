use std::path::Path;

use super::super::diagnostic::{LintDiagnostic, Severity};

const DEBUG_CALL_RULE_ID: &str = "TL0904";
const DEBUG_CALL_RULE_NAME: &str = "maintainability.debug-call";

/// Builds diagnostics for unstructured debug calls left in production source.
pub(super) fn debug_call_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    if is_test_source_path(path) {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some(column) = first_unstructured_debug_call_column(line) else {
            continue;
        };
        diagnostics.push(LintDiagnostic {
            path: path.to_path_buf(),
            line: line_index + 1,
            column,
            rule_id: DEBUG_CALL_RULE_ID,
            rule_name: DEBUG_CALL_RULE_NAME,
            severity: Severity::Warning,
            message: "debug-style calls in production source need structured logging or diagnostic intent",
            fix_available: false,
        });
    }

    diagnostics
}

fn is_test_source_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|part| part == "tests" || part == "test")
    }) || path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("Test.terl") || name.ends_with("Test.terli"))
}

fn first_unstructured_debug_call_column(line: &str) -> Option<usize> {
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
        if ch == '(' {
            if let Some(start) = call_name_start(line, index) {
                let call_name = line[start..index].trim();
                if is_unstructured_debug_call(call_name) {
                    return Some(start + 1);
                }
            }
        }
        index += 1;
    }

    None
}

fn call_name_start(line: &str, open_paren_index: usize) -> Option<usize> {
    let prefix = line[..open_paren_index].trim_end();
    if prefix.is_empty() {
        return None;
    }
    let end = prefix.len();
    let start = prefix[..end]
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .map_or(0, |index| index + 1);
    (start < end).then_some(start)
}

fn is_unstructured_debug_call(call_name: &str) -> bool {
    if call_name.starts_with("Logger.") || call_name.starts_with("Log.") {
        return false;
    }
    matches!(
        call_name.rsplit('.').next(),
        Some("debug" | "dbg" | "dump" | "inspect")
    )
}
