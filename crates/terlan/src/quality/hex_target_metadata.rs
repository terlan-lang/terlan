use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the Terlan package metadata contract.
const PACKAGE_METADATA_DOC: &str = "docs/package/TERLAN_PACKAGE_METADATA.md";

const REQUIRED_TERMS: &[&str] = &[
    "target-neutral",
    "terlan.toml",
    "terlan-package-build.json",
    "package identity",
    "dependencies",
    "source roots",
    "capabilities",
    "target profiles",
    "generated artifacts",
    "native boundary declarations",
    "compiler target selection",
    "hex may be used as distribution infrastructure",
    "does not imply otp compatibility",
    "rebar compatibility",
    "beam bytecode compatibility",
    "terlan-owned manifests and lockfiles define the compiler contract",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "hex is the package contract",
    "hex is the authoritative source",
    "hex implies otp",
    "hex implies beam",
    "rebar is required",
    "otp application boot compatibility is required",
];
const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the Hex target metadata gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexTargetMetadataSummary {
    pub required_term_count: usize,
}

/// Runs the Hex target metadata contract gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary when the package metadata contract is target-neutral.
/// - Stable diagnostics when required terms are missing or Hex is described as
///   OTP/Rebar/VM compatibility scope.
///
/// Transformation:
/// - Reads the checked-in package metadata contract and validates the
///   target-neutral Hex distribution wording needed by the 0.0.7 package pivot.
pub fn run_hex_target_metadata(root: &Path) -> QualityResult<HexTargetMetadataSummary> {
    let path = root.join(PACKAGE_METADATA_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package metadata contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_hex_target_metadata_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(HexTargetMetadataSummary {
        required_term_count: REQUIRED_TERMS.len(),
    })
}

/// Validates package metadata contract text.
fn validate_hex_target_metadata_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(term) {
            diagnostics.push(format!("missing package metadata contract term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden package metadata claim `{claim}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package metadata term `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Renders package metadata diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[hex-target-metadata] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "hex_target_metadata_test.rs"]
mod hex_target_metadata_test;
