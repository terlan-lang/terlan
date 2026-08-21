//! Deterministic offline audit of a resolved Registry lock and local cache.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use crate::DiagnosticFormat;

use super::package_registry_error::RegistryResult;
use super::package_registry_resolver::{read_lockfile, LockedRegistryPackage};
use super::package_registry_transport::sha256_hex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AuditFinding {
    severity: &'static str,
    code: &'static str,
    package: Option<String>,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AuditPackage {
    name: String,
    version: String,
    targets: Vec<String>,
    capabilities: Vec<String>,
    dependency_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AuditSummary {
    schema: &'static str,
    network_access: &'static str,
    security_advisories: &'static str,
    packages: Vec<AuditPackage>,
    findings: Vec<AuditFinding>,
}

pub(super) fn run(args: &[String], output_root: &Path, format: DiagnosticFormat) -> ExitCode {
    if args != ["audit"] {
        eprintln!("usage: terlc package audit --out-dir <project-dir>");
        return ExitCode::from(2);
    }
    let summary = match audit(output_root) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    match format {
        DiagnosticFormat::Json => match serde_json::to_string_pretty(&summary) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("error[registry_audit_json]: {error}");
                return ExitCode::from(1);
            }
        },
        DiagnosticFormat::Text { .. } => render_text(&summary),
    }
    if summary
        .findings
        .iter()
        .any(|finding| finding.severity == "error")
    {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn audit(output_root: &Path) -> RegistryResult<AuditSummary> {
    let lock = read_lockfile(&output_root.join("terlan.lock"))?;
    let mut findings = Vec::new();
    let mut versions = BTreeMap::<String, BTreeSet<String>>::new();
    let mut packages = Vec::with_capacity(lock.registry.len());
    for entry in &lock.registry {
        versions
            .entry(entry.name.clone())
            .or_default()
            .insert(entry.version.clone());
        audit_entry(output_root, entry, &mut findings);
        packages.push(AuditPackage {
            name: entry.name.clone(),
            version: entry.version.clone(),
            targets: entry.targets.clone(),
            capabilities: entry.capabilities.clone(),
            dependency_count: entry.dependencies.len(),
        });
    }
    for (name, versions) in versions {
        if versions.len() > 1 {
            findings.push(AuditFinding {
                severity: "warning",
                code: "duplicate_versions",
                package: Some(name.clone()),
                message: format!(
                    "{name} is locked at multiple versions: {}",
                    versions.into_iter().collect::<Vec<_>>().join(", ")
                ),
            });
        }
    }
    packages.sort_by(|left, right| (&left.name, &left.version).cmp(&(&right.name, &right.version)));
    findings.sort_by(|left, right| {
        (&left.severity, &left.code, &left.package, &left.message).cmp(&(
            &right.severity,
            &right.code,
            &right.package,
            &right.message,
        ))
    });
    Ok(AuditSummary {
        schema: "terlan-package-audit-v1",
        network_access: "disabled",
        security_advisories: "unavailable",
        packages,
        findings,
    })
}

fn audit_entry(
    output_root: &Path,
    entry: &LockedRegistryPackage,
    findings: &mut Vec<AuditFinding>,
) {
    let expected_source = format!("registry:{}/{}", entry.registry, entry.name);
    if entry.source_identity != expected_source {
        findings.push(AuditFinding {
            severity: "error",
            code: "source_path_leakage",
            package: Some(entry.name.clone()),
            message: "locked source identity is not the canonical Registry identity".into(),
        });
    }
    if entry.cache_key != entry.archive_sha256 {
        findings.push(AuditFinding {
            severity: "error",
            code: "cache_key_mismatch",
            package: Some(entry.name.clone()),
            message: "cache key does not equal the locked archive digest".into(),
        });
        return;
    }
    let archive = output_root
        .join(".terlan/packages/registry")
        .join(&entry.cache_key)
        .join("archive.tar.zst");
    let bytes = match fs::read(&archive) {
        Ok(bytes) => bytes,
        Err(error) => {
            findings.push(AuditFinding {
                severity: "error",
                code: "cache_entry_missing",
                package: Some(entry.name.clone()),
                message: format!("cannot read cached archive: {error}"),
            });
            return;
        }
    };
    if sha256_hex(&bytes) != entry.archive_sha256 {
        findings.push(AuditFinding {
            severity: "error",
            code: "cache_poisoning",
            package: Some(entry.name.clone()),
            message: "cached archive digest does not match the lockfile".into(),
        });
    }
}

fn render_text(summary: &AuditSummary) {
    println!(
        "audited {} locked package(s); network access is disabled",
        summary.packages.len()
    );
    for package in &summary.packages {
        println!("{}@{}", package.name, package.version);
    }
    for finding in &summary.findings {
        println!(
            "{}[{}]: {}",
            finding.severity, finding.code, finding.message
        );
    }
}

#[cfg(test)]
#[path = "package_registry_audit_test.rs"]
mod tests;
