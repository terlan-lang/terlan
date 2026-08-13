use std::fs;
use std::path::Path;

use serde_json::json;

use crate::terlan_quality::QualityResult;

const PACKAGE_CAPABILITY_DOC: &str = "docs/package/TERLAN_PACKAGE_CAPABILITY_CONTRACT.md";
const REPORT_PATH: &str = "target/quality/package-capability-contract-report.json";

const REQUIRED_TERMS: &[&str] = &[
    "filesystem",
    "network",
    "HTTP listener",
    "database",
    "NativeBoundary resources",
    "generated bindings",
    "native artifacts",
    "environment variables",
    "process spawning",
    "debugger hooks",
    "release-time hooks",
    "install",
    "build",
    "typecheck",
    "VM runtime",
    "release packaging",
    "support-bundle generation",
    "No ambient permissions",
    "resource handle types",
    "blocking policy",
    "cancellation behavior",
    "target compatibility",
    "generated binding hash",
    "native artifact hash",
    "security review status",
    "deterministic capability summary",
    "lockfiles",
    "diagnostics",
    "generated docs",
    "release reports",
    "package-capability-contract-report.json",
    "package capability matrix",
    "denied operation fixtures",
    "native resource inventory",
    "lockfile capability hashes",
    "diagnostic coverage",
    "undeclared filesystem access",
    "undeclared network access",
    "hidden NativeBoundary calls",
    "stale native artifact hashes",
    "capability drift between manifest and lockfile",
    "package import aliases bypassing checks",
    "generated bindings requesting extra capabilities",
    "runtime handles reused across package boundaries",
];

const FORBIDDEN_CLAIMS: &[&str] = &[
    "packages inherit ambient permissions",
    "undeclared capabilities are allowed",
    "native calls inherit host permissions",
    "support bundles include privileged data by default",
    "aliases bypass capability checks",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

/// Summary produced by the package capability contract gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCapabilityContractSummary {
    pub required_term_count: usize,
    pub forbidden_claim_count: usize,
    pub report_path: String,
}

/// Runs the package capability contract gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and a machine-readable report when package capability
///   surfaces and adversarial cases are documented.
/// - Stable diagnostics when capability declarations, runtime checkpoints, or
///   native package metadata are missing.
///
/// Transformation:
/// - Converts the package capability contract into executable release evidence
///   for the 0.0.7 package-system roadmap.
pub fn run_package_capability_contract(
    root: &Path,
) -> QualityResult<PackageCapabilityContractSummary> {
    let path = root.join(PACKAGE_CAPABILITY_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read package capability contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_package_capability_contract_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path)?;
    Ok(PackageCapabilityContractSummary {
        required_term_count: REQUIRED_TERMS.len(),
        forbidden_claim_count: FORBIDDEN_CLAIMS.len(),
        report_path: REPORT_PATH.to_string(),
    })
}

fn validate_package_capability_contract_text(text: &str) -> Vec<String> {
    let normalized = text.to_lowercase();
    let mut diagnostics = Vec::new();
    for term in REQUIRED_TERMS {
        if !normalized.contains(&term.to_lowercase()) {
            diagnostics.push(format!("missing package capability contract term `{term}`"));
        }
    }
    for claim in FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!(
                "forbidden package capability contract claim `{claim}`"
            ));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(placeholder) {
            diagnostics.push(format!(
                "placeholder package capability contract text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

fn write_report(report_path: &Path) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package capability report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-capability-contract.v1",
        "capability_evidence": "explicit package capability manifest contract",
        "capability_surfaces": [
            "filesystem",
            "network",
            "HTTP listener",
            "database",
            "NativeBoundary resources",
            "generated bindings",
            "native artifacts",
            "environment variables",
            "process spawning",
            "debugger hooks",
            "release-time hooks"
        ],
        "checkpoints": [
            "install",
            "build",
            "typecheck",
            "VM runtime",
            "release packaging",
            "support-bundle generation"
        ],
        "native_package_metadata": [
            "resource handle types",
            "blocking policy",
            "cancellation behavior",
            "target compatibility",
            "generated binding hash",
            "native artifact hash",
            "security review status"
        ],
        "adversarial_cases": [
            "undeclared filesystem access",
            "undeclared network access",
            "hidden NativeBoundary calls",
            "stale native artifact hashes",
            "capability drift between manifest and lockfile",
            "package import aliases bypassing checks",
            "generated bindings requesting extra capabilities",
            "runtime handles reused across package boundaries"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package capability report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package capability report: {err}",
            report_path.display()
        )
    })
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[package-capability-contract] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "package_capability_contract_test.rs"]
#[cfg(test)]
mod package_capability_contract_test;
