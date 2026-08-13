use std::fs;
use std::path::Path;

use serde_json::json;

use super::package_git_source::run_package_git_source;
use super::package_lockfile_contract::run_package_lockfile_contract;
use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/package-resolver-reproducibility-report.json";

/// Summary produced by the package resolver reproducibility gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageResolverReproducibilitySummary {
    pub lockfile_term_count: usize,
    pub lockfile_field_count: usize,
    pub git_source_term_count: usize,
    pub git_source_field_count: usize,
    pub report_path: String,
}

/// Runs the package resolver reproducibility gate.
///
/// Inputs:
/// - `root`: repository root containing `docs/package/`.
///
/// Output:
/// - Success summary and a machine-readable report when package lockfile and
///   Git source contracts both pass.
/// - Stable diagnostics from the underlying package-source validators when
///   either contract drifts.
///
/// Transformation:
/// - Composes the lockfile and Git-source package gates into the resolver-level
///   reproducibility evidence required by the 0.0.7 package-system roadmap.
pub fn run_package_resolver_reproducibility(
    root: &Path,
) -> QualityResult<PackageResolverReproducibilitySummary> {
    let lockfile = run_package_lockfile_contract(root)?;
    let git_source = run_package_git_source(root)?;
    let report_path = root.join(REPORT_PATH);
    write_report(&report_path, &lockfile, &git_source)?;
    Ok(PackageResolverReproducibilitySummary {
        lockfile_term_count: lockfile.required_term_count,
        lockfile_field_count: lockfile.required_field_count,
        git_source_term_count: git_source.required_term_count,
        git_source_field_count: git_source.required_field_count,
        report_path: REPORT_PATH.to_string(),
    })
}

fn write_report(
    report_path: &Path,
    lockfile: &super::package_lockfile_contract::PackageLockfileContractSummary,
    git_source: &super::package_git_source::PackageGitSourceSummary,
) -> QualityResult<()> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create package resolver report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan.package-resolver-reproducibility.v1",
        "resolver_evidence": "contract-backed package source reproducibility",
        "lockfile_contract": {
            "required_terms": lockfile.required_term_count,
            "required_fields": lockfile.required_field_count
        },
        "git_source_contract": {
            "required_terms": git_source.required_term_count,
            "required_fields": git_source.required_field_count
        },
        "deterministic_run_comparison": {
            "status": "contract-enforced",
            "inputs": [
                "terlan.lock contract",
                "immutable Git source contract"
            ]
        },
        "diagnostic_coverage": [
            "missing lockfile terms",
            "missing lockfile fields",
            "forbidden target lockfile authority",
            "missing Git source terms",
            "missing Git source fields",
            "forbidden floating Git authority"
        ]
    });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize package resolver report: {err}"))?;
    fs::write(report_path, format!("{text}\n")).map_err(|err| {
        format!(
            "{}: failed to write package resolver report: {err}",
            report_path.display()
        )
    })
}

#[cfg(test)]
#[path = "package_resolver_reproducibility_test.rs"]
#[cfg(test)]
mod package_resolver_reproducibility_test;
