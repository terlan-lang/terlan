use crate::terlan_syntax::{
    span::Span, SyntaxConfigEntryOutput, SyntaxDeclarationPayload, SyntaxModuleOutput,
};
use crate::terlan_typeck::{DiagSeverity, Diagnostic};

/// Checks config declarations in one syntax-output module.
///
/// Inputs:
/// - `module`: formal syntax-output module containing parsed declarations.
///
/// Output:
/// - Warning diagnostics for config declarations whose preserved text contains
///   structured metadata entries not consumed by the generic compiler path.
///
/// Transformation:
/// - Filters syntax declarations to config payloads, checks the structured
///   config entry list produced by syntax output, and maps unsupported semantic
///   consumption to warning diagnostics.
pub(crate) fn check_config_declarations_syntax_output(
    module: &SyntaxModuleOutput,
) -> Vec<Diagnostic> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Config {
                name,
                target,
                entries,
                ..
            } = &declaration.payload
            else {
                return None;
            };
            if !has_structured_config_entries(entries) {
                return None;
            }
            Some(config_entries_preserved_warning(
                name,
                target,
                declaration.span.into(),
            ))
        })
        .collect()
}

/// Detects whether preserved config text contains structured entries.
///
/// Inputs:
/// - `entries`: structured config entries from syntax output.
///
/// Output:
/// - `true` when at least one structured metadata entry exists.
///
/// Transformation:
/// - Treats the syntax-output structured payload as authoritative and avoids
///   reparsing preserved source text in CLI validation.
fn has_structured_config_entries(entries: &[SyntaxConfigEntryOutput]) -> bool {
    !entries.is_empty()
}

/// Builds the generic config-entry preservation warning.
///
/// Inputs:
/// - `name`: config declaration head, such as `target` or `native`.
/// - `target`: config target path, such as `erlang` or `js`.
/// - `span`: source span for the config declaration.
///
/// Output:
/// - Warning diagnostic describing the 0.0.1 config semantic boundary.
///
/// Transformation:
/// - Formats declaration identity into a stable CLI diagnostic and marks it as a
///   warning so later phases can continue when no target-specific validator
///   rejects the declaration.
fn config_entries_preserved_warning(name: &str, target: &str, span: Span) -> Diagnostic {
    Diagnostic {
        span,
        message: format!(
            "config metadata entries for `{name} {target}` are preserved but not semantically consumed by the generic 0.0.1 compiler path; target-specific validators must opt in before backend behavior depends on them"
        ),
        severity: DiagSeverity::Warning,
    }
}

#[cfg(test)]
#[path = "config_contract_test.rs"]
mod config_contract_test;
