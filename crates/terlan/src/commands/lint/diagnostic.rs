use std::path::PathBuf;

/// A stable lint finding emitted by `terlc lint`.
///
/// Inputs:
/// - Source path, line/column coordinates, and rule-owned message metadata.
///
/// Output:
/// - Human-readable diagnostics with stable rule IDs and fix markers.
///
/// Transformation:
/// - Keeps command output independent from internal parser data structures so
///   lint can start with source-text rules before semantic rules are added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LintDiagnostic {
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) rule_id: &'static str,
    pub(super) rule_name: &'static str,
    pub(super) severity: Severity,
    pub(super) message: &'static str,
    pub(super) fix_available: bool,
}

/// Lint diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Severity {
    Error,
    Warning,
    Suggestion,
}

impl Severity {
    /// Returns the public diagnostic severity label.
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Suggestion => "suggestion",
        }
    }
}

/// Renders one diagnostic in the stable text format.
pub(super) fn render_diagnostic(diagnostic: &LintDiagnostic) -> String {
    let fix = if diagnostic.fix_available {
        " [fix available]"
    } else {
        ""
    };
    format!(
        "{}:{}:{}: {}[{}:{}]: {}{}",
        diagnostic.path.display(),
        diagnostic.line,
        diagnostic.column,
        diagnostic.severity.as_str(),
        diagnostic.rule_id,
        diagnostic.rule_name,
        diagnostic.message,
        fix
    )
}
