use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Terlan lockfile contract.
const PACKAGE_LOCKFILE_DOC: &str = "docs/package/TERLAN_PACKAGE_LOCKFILE.md";

const REQUIRED_TERMS: &[&str] = &[
    "terlan.lock",
    "terlan-owned dependency resolution artifact",
    "compiler contract",
    "reproducible package resolution",
    "local path dependencies",
    "git dependencies",
    "immutable `rev`",
    "optional static index",
    "hex, npm, and cargo dependencies",
    "must be deterministic",
    "target package manager lockfiles",
    "secondary to `terlan.lock`",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "cargo.lock is the terlan lockfile",
    "package-lock.json is the terlan lockfile",
    "rebar.lock is the terlan lockfile",
    "hex lock is authoritative",
];

/// Summary produced by the package lockfile contract gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLockfileContractSummary {
    pub required_term_count: usize,
}

/// Runs the package lockfile contract gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary when the lockfile contract is present.
/// - Stable diagnostics when required terms are missing or a target package
///   manager lockfile is made authoritative.
///
/// Transformation:
/// - Reads the checked-in lockfile contract and validates Terlan-owned
///   lockfile semantics for the 0.0.7 package-source pivot.
pub fn run_package_lockfile_contract(root: &Path) -> QualityResult<PackageLockfileContractSummary> {
    let path = root.join(PACKAGE_LOCKFILE_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package lockfile contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_lockfile_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(PackageLockfileContractSummary {
        required_term_count: REQUIRED_TERMS.len(),
    })
}

/// Validates package lockfile contract text.
fn validate_package_lockfile_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!("missing package lockfile contract term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package lockfile claim `{claim}`"));
        }
    }
    diagnostics
}

/// Renders package lockfile diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-lockfile-contract] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_lockfile_contract_test.rs"]
mod package_lockfile_contract_test;
