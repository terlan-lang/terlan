use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::terlan_quality::lean_proof_track::lean_proof_gap::{parse_gap_manifest, GAP_PATH};
use crate::terlan_quality::{render_failure, QualityResult};

const INVENTORY_PATH: &str = "docs/compiler/proof_track/lean_proof_inventory.tsv";
const OWNER_PATH: &str = "docs/compiler/proof_track/lean_proof_owners.tsv";
const REPORT_PATH: &str = "build/artifacts/lean-proof-pr-report.json";

const INVENTORY_HEADER: &str = "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes";
const OWNER_HEADER: &str = "subject_type\tsubject\towner_bucket\tfeature_slices\trequired_gates\tnext_action\texception_token\texception_expiry";
const ALLOWED_OWNER_BUCKETS: &[&str] = &["cli", "vm", "std", "db", "templates", "runtime"];

/// Summary produced by the Lean proof PR/ownership gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanProofPrSummary {
    pub owner_count: usize,
    pub unresolved_gap_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InventoryRow {
    path: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GapRow {
    feature: String,
    planned_gate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OwnerRow {
    subject_type: String,
    subject: String,
    owner_bucket: String,
    feature_slices: Vec<String>,
    required_gates: Vec<String>,
    next_action: String,
    exception_token: String,
    exception_expiry: String,
}

#[derive(Debug, Serialize)]
struct LeanProofPrReport {
    changed_feature_classes: Vec<String>,
    proof_deltas: Vec<String>,
    gap_delta_reasons: Vec<String>,
    owners: Vec<OwnerReport>,
}

#[derive(Debug, Serialize)]
struct OwnerReport {
    subject_type: String,
    subject: String,
    owner_bucket: String,
    feature_slices: Vec<String>,
    required_gates: Vec<String>,
    next_action: String,
    exception: Option<ExceptionReport>,
}

#[derive(Debug, Serialize)]
struct ExceptionReport {
    token: String,
    expiry: String,
}

/// Runs Lean proof PR observability and ownership validation.
///
/// Inputs:
/// - `root`: repository root containing proof inventory, proof gaps, owner map,
///   and Makefile.
///
/// Output:
/// - Success summary with owner rows, unresolved gap rows, and PR report path.
/// - Stable diagnostics for missing owners, unknown buckets, stale gate links,
///   or malformed exception metadata.
///
/// Transformation:
/// - Turns proof status into owner-specific PR/release evidence without treating
///   unresolved gaps as complete proof coverage.
pub fn run_lean_proof_pr(root: &Path) -> QualityResult<LeanProofPrSummary> {
    let inventory = parse_inventory(&read_text(root, INVENTORY_PATH)?)?;
    let gaps = parse_gaps(&read_text(root, GAP_PATH)?)?;
    let owners = parse_owners(&read_text(root, OWNER_PATH)?)?;
    let make_targets = collect_make_targets(&read_text(root, "Makefile")?);

    let mut diagnostics = validate_owners(&owners, &make_targets);
    diagnostics.extend(validate_inventory_ownership(&inventory, &owners));
    diagnostics.extend(validate_gap_ownership(&gaps, &owners));

    if !diagnostics.is_empty() {
        return Err(render_failure("lean-proof-pr", &diagnostics));
    }

    let report_path = write_report(root, &build_report(&owners, &gaps))?;
    Ok(LeanProofPrSummary {
        owner_count: owners.len(),
        unresolved_gap_count: gaps.len(),
        report_path,
    })
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

fn parse_inventory(text: &str) -> QualityResult<Vec<InventoryRow>> {
    Ok(parse_tsv(text, INVENTORY_HEADER, INVENTORY_PATH)?
        .into_iter()
        .map(|columns| InventoryRow {
            path: columns[0].clone(),
            status: columns[1].clone(),
        })
        .collect())
}

fn parse_gaps(text: &str) -> QualityResult<Vec<GapRow>> {
    Ok(parse_gap_manifest(text)?
        .into_iter()
        .map(|gap| GapRow {
            feature: gap.feature,
            planned_gate: gap.planned_gate,
        })
        .collect())
}

fn parse_owners(text: &str) -> QualityResult<Vec<OwnerRow>> {
    Ok(parse_tsv(text, OWNER_HEADER, OWNER_PATH)?
        .into_iter()
        .map(|columns| OwnerRow {
            subject_type: columns[0].clone(),
            subject: columns[1].clone(),
            owner_bucket: columns[2].clone(),
            feature_slices: split_list(&columns[3]),
            required_gates: split_list(&columns[4]),
            next_action: columns[5].clone(),
            exception_token: columns[6].clone(),
            exception_expiry: columns[7].clone(),
        })
        .collect())
}

fn parse_tsv(text: &str, header: &str, path: &str) -> QualityResult<Vec<Vec<String>>> {
    let mut lines = text.lines();
    let Some(actual_header) = lines.next() else {
        return Err(format!("{path}: missing header"));
    };
    if actual_header != header {
        return Err(format!(
            "{path}: expected header `{header}`, found `{actual_header}`"
        ));
    }
    let expected_columns = header.split('\t').count();
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if columns.len() != expected_columns {
            return Err(format!(
                "{path}: row {} has {} columns, expected {expected_columns}",
                index + 2,
                columns.len()
            ));
        }
        rows.push(columns);
    }
    Ok(rows)
}

fn split_list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn collect_make_targets(makefile: &str) -> BTreeSet<String> {
    makefile
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_end();
            if trimmed.starts_with('\t')
                || trimmed.starts_with('.')
                || trimmed.starts_with('#')
                || trimmed.is_empty()
            {
                return None;
            }
            let (name, _) = trimmed.split_once(':')?;
            Some(name.trim().to_string())
        })
        .collect()
}

fn validate_owners(owners: &[OwnerRow], make_targets: &BTreeSet<String>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for owner in owners {
        let key = (owner.subject_type.as_str(), owner.subject.as_str());
        if !seen.insert(key) {
            diagnostics.push(format!(
                "{OWNER_PATH}: duplicate owner row for `{}` `{}`",
                owner.subject_type, owner.subject
            ));
        }
        if !matches!(owner.subject_type.as_str(), "inventory" | "gap") {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` has unknown subject_type `{}`",
                owner.subject, owner.subject_type
            ));
        }
        if !ALLOWED_OWNER_BUCKETS.contains(&owner.owner_bucket.as_str()) {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` owner `{}` is not a canonical proof owner bucket",
                owner.subject, owner.owner_bucket
            ));
        }
        if owner.feature_slices.is_empty() {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` must link at least one feature slice",
                owner.subject
            ));
        }
        if owner.required_gates.is_empty() {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` must link at least one required gate",
                owner.subject
            ));
        }
        for gate in &owner.required_gates {
            if !make_targets.contains(gate) {
                diagnostics.push(format!(
                    "{OWNER_PATH}: subject `{}` references unknown Make gate `{gate}`",
                    owner.subject
                ));
            }
        }
        if owner.next_action.trim().is_empty() {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` must include an owner next action",
                owner.subject
            ));
        }
        if (owner.exception_token == "none") != (owner.exception_expiry == "none") {
            diagnostics.push(format!(
                "{OWNER_PATH}: subject `{}` exception token and expiry must both be `none` or both concrete",
                owner.subject
            ));
        }
    }
    diagnostics
}

fn validate_inventory_ownership(inventory: &[InventoryRow], owners: &[OwnerRow]) -> Vec<String> {
    let owner_subjects = owner_subjects(owners, "inventory");
    inventory
        .iter()
        .filter(|row| row.status == "current" || row.status == "absent")
        .filter(|row| !owner_subjects.contains(row.path.as_str()))
        .map(|row| format!("{OWNER_PATH}: inventory row `{}` has no owner", row.path))
        .collect()
}

fn validate_gap_ownership(gaps: &[GapRow], owners: &[OwnerRow]) -> Vec<String> {
    let owner_subjects = owner_subjects(owners, "gap");
    let owner_by_subject = owners
        .iter()
        .filter(|owner| owner.subject_type == "gap")
        .map(|owner| (owner.subject.as_str(), owner))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();
    for gap in gaps {
        if !owner_subjects.contains(gap.feature.as_str()) {
            diagnostics.push(format!(
                "{OWNER_PATH}: unresolved gap `{}` has no owner",
                gap.feature
            ));
            continue;
        }
        let owner = owner_by_subject
            .get(gap.feature.as_str())
            .expect("checked above");
        if !owner
            .required_gates
            .iter()
            .any(|gate| gate == &gap.planned_gate)
        {
            diagnostics.push(format!(
                "{OWNER_PATH}: gap `{}` owner row must include planned gate `{}`",
                gap.feature, gap.planned_gate
            ));
        }
    }
    diagnostics
}

fn owner_subjects<'a>(owners: &'a [OwnerRow], subject_type: &str) -> BTreeSet<&'a str> {
    owners
        .iter()
        .filter(|owner| owner.subject_type == subject_type)
        .map(|owner| owner.subject.as_str())
        .collect()
}

fn build_report(owners: &[OwnerRow], gaps: &[GapRow]) -> LeanProofPrReport {
    LeanProofPrReport {
        changed_feature_classes: Vec::new(),
        proof_deltas: Vec::new(),
        gap_delta_reasons: gaps
            .iter()
            .map(|gap| format!("{} remains unresolved", gap.feature))
            .collect(),
        owners: owners
            .iter()
            .map(|owner| OwnerReport {
                subject_type: owner.subject_type.clone(),
                subject: owner.subject.clone(),
                owner_bucket: owner.owner_bucket.clone(),
                feature_slices: owner.feature_slices.clone(),
                required_gates: owner.required_gates.clone(),
                next_action: owner.next_action.clone(),
                exception: if owner.exception_token == "none" {
                    None
                } else {
                    Some(ExceptionReport {
                        token: owner.exception_token.clone(),
                        expiry: owner.exception_expiry.clone(),
                    })
                },
            })
            .collect(),
    }
}

fn write_report(root: &Path, report: &LeanProofPrReport) -> QualityResult<PathBuf> {
    let path = root.join(REPORT_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create directory: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|err| format!("{}: failed to serialize report: {err}", path.display()))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|err| format!("{}: failed to write report: {err}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
#[path = "lean_proof_pr_test.rs"]
mod lean_proof_pr_test;
