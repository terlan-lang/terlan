use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Git source package contract.
const PACKAGE_GIT_SOURCE_DOC: &str = "docs/package/TERLAN_PACKAGE_GIT_SOURCE.md";

const REQUIRED_TERMS: &[&str] = &[
    "git source dependencies",
    "url",
    "immutable `rev`",
    "floating branches and tags",
    "resolution input",
    "terlan.lock",
    "release builds",
    "must be deterministic",
    "implicit network",
    "local path dependencies",
    "target package manager metadata",
    "secondary",
];

const REQUIRED_SOURCE_FIELDS: &[&str] = &[
    "dependency name",
    "repository url",
    "immutable revision",
    "resolved revision checksum",
    "lockfile entry",
    "resolver version",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "branch is authoritative",
    "tag is authoritative",
    "latest commit is authoritative",
    "cargo git resolution is authoritative",
    "implicit network resolution is allowed for release builds",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package Git source contract gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageGitSourceSummary {
    pub required_term_count: usize,
    pub required_field_count: usize,
}

/// Runs the package Git source contract gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary when Git source semantics are documented.
/// - Stable diagnostics when required terms are missing or floating Git state
///   is made authoritative.
///
/// Transformation:
/// - Reads the checked-in Git source contract and validates deterministic
///   package-source semantics for release builds.
pub fn run_package_git_source(root: &Path) -> QualityResult<PackageGitSourceSummary> {
    let path = root.join(PACKAGE_GIT_SOURCE_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package Git source contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_git_source_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(PackageGitSourceSummary {
        required_term_count: REQUIRED_TERMS.len(),
        required_field_count: REQUIRED_SOURCE_FIELDS.len(),
    })
}

/// Validates package Git source contract text.
fn validate_package_git_source_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!("missing package Git source contract term `{term}`"));
        }
    }
    for field in REQUIRED_SOURCE_FIELDS {
        if !normalized.contains(field) {
            diagnostics.push(format!("missing package Git source field `{field}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package Git source claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package Git source text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Renders package Git source diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-git-source] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_git_source_test.rs"]
#[cfg(test)]
mod package_git_source_test;
