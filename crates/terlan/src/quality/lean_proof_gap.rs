use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use time::{Date, Month, OffsetDateTime};

use crate::terlan_quality::QualityResult;

pub(crate) const GAP_PATH: &str = "docs/compiler/proof_track/lean_proof_gaps.tsv";
pub(crate) const GAP_HEADER: &str = "feature\tlifecycle_status\tproof_gap_category\tgap_reason\tremediation_owner\tplanned_gate\tdeadline_or_exception\tblocker_updated_at\tblocker_hash\tcovered_manifests";
pub(crate) const GAP_TOML_DIR: &str = "proofs/lean/gaps";
const GAP_POLICY_PATH: &str = "docs/compiler/proof_track/lean_proof_gap_policy.toml";
const GATE_REPORT_PATH: &str = "build/artifacts/lean-proof-gate.json";

const LIFECYCLE_STATUSES: &[&str] = &["open", "triaged", "blocked", "remediated", "closed"];
const GAP_CATEGORIES: &[&str] = &[
    "not_started",
    "resource",
    "model_gap",
    "performance",
    "toolchain",
];
const PROOF_LANES: &[&str] = &[
    "parser",
    "coreir",
    "target_profile",
    "vm_runtime",
    "native_boundary",
    "wasm",
    "distribution",
    "std_packages",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LeanProofGap {
    pub feature: String,
    pub lifecycle_status: String,
    pub category: String,
    pub reason: String,
    pub remediation_owner: String,
    pub planned_gate: String,
    pub deadline_or_exception: String,
    pub blocker_updated_at: String,
    pub blocker_hash: String,
    pub covered_manifests: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeanProofGapFile {
    schema: String,
    feature: String,
    lifecycle_status: String,
    proof_gap_category: String,
    gap_reason: String,
    remediation_owner: String,
    planned_gate: String,
    deadline_or_exception: String,
    exception_approved_by: String,
    blocker_updated_at: String,
    blocker_hash: String,
    covered_manifests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct GapPolicy {
    schema: String,
    max_blocker_age_days: i64,
}

#[derive(Debug, Serialize)]
struct GapMetric {
    feature: String,
    lifecycle_status: String,
    gap_staleness_days: i64,
    gap_classification_confidence: f64,
}

pub(crate) fn parse_gap_manifest(text: &str) -> QualityResult<Vec<LeanProofGap>> {
    let mut lines = text.lines();
    match lines.next() {
        Some(header) if header == GAP_HEADER => {}
        Some(header) => {
            return Err(format!(
                "`{GAP_PATH}` header mismatch: expected `{GAP_HEADER}`, found `{header}`"
            ));
        }
        None => return Err(format!("`{GAP_PATH}` is empty")),
    }

    let width = GAP_HEADER.split('\t').count();
    let mut gaps = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != width {
            return Err(format!(
                "`{GAP_PATH}` row {} has {} columns, expected {width}",
                index + 2,
                columns.len()
            ));
        }
        gaps.push(LeanProofGap {
            feature: columns[0].to_string(),
            lifecycle_status: columns[1].to_string(),
            category: columns[2].to_string(),
            reason: columns[3].to_string(),
            remediation_owner: columns[4].to_string(),
            planned_gate: columns[5].to_string(),
            deadline_or_exception: columns[6].to_string(),
            blocker_updated_at: columns[7].to_string(),
            blocker_hash: columns[8].to_string(),
            covered_manifests: split_list(columns[9]),
        });
    }
    Ok(gaps)
}

/// Validates per-gap TOML as the strict lifecycle source mirrored by the TSV index.
pub(crate) fn validate_gap_toml_mirror(
    root: &Path,
    indexed_gaps: &[LeanProofGap],
    today: Date,
) -> QualityResult<Vec<String>> {
    let directory = root.join(GAP_TOML_DIR);
    let entries = fs::read_dir(&directory).map_err(|error| {
        format!(
            "{}: failed to read gap directory: {error}",
            directory.display()
        )
    })?;
    let mut paths = entries
        .map(|entry| {
            entry.map(|entry| entry.path()).map_err(|error| {
                format!("{}: failed to read gap entry: {error}", directory.display())
            })
        })
        .collect::<QualityResult<Vec<_>>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "toml")
    });
    paths.sort();
    if paths.is_empty() {
        return Ok(vec![format!(
            "`{GAP_TOML_DIR}` must contain at least one per-gap TOML manifest"
        )]);
    }

    let indexed = indexed_gaps
        .iter()
        .map(|gap| (gap.feature.as_str(), gap))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut seen = std::collections::BTreeSet::new();
    let mut diagnostics = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("{relative}: failed to read gap TOML: {error}"))?;
        let record = basic_toml::from_str::<LeanProofGapFile>(&text)
            .map_err(|error| format!("{relative}: invalid gap TOML: {error}"))?;
        if record.schema != "terlan.lean-proof-gap.v1" {
            diagnostics.push(format!(
                "`{relative}` has unsupported schema `{}`",
                record.schema
            ));
        }
        if !seen.insert(record.feature.clone()) {
            diagnostics.push(format!(
                "`{GAP_TOML_DIR}` contains duplicate feature `{}`",
                record.feature
            ));
        }
        let expected_file = format!("{}.toml", gap_file_stem(&record.feature));
        if path.file_name().and_then(|name| name.to_str()) != Some(expected_file.as_str()) {
            diagnostics.push(format!(
                "`{relative}` must use canonical filename `{expected_file}`"
            ));
        }
        if record.exception_approved_by != record.remediation_owner {
            diagnostics.push(format!(
                "`{relative}` exception approver `{}` must match remediation owner `{}`",
                record.exception_approved_by, record.remediation_owner
            ));
        }
        diagnostics.extend(validate_lane_exception(
            &relative,
            &record.deadline_or_exception,
            today,
        ));
        let gap = LeanProofGap {
            feature: record.feature,
            lifecycle_status: record.lifecycle_status,
            category: record.proof_gap_category,
            reason: record.gap_reason,
            remediation_owner: record.remediation_owner,
            planned_gate: record.planned_gate,
            deadline_or_exception: record.deadline_or_exception,
            blocker_updated_at: record.blocker_updated_at,
            blocker_hash: record.blocker_hash,
            covered_manifests: record.covered_manifests,
        };
        match indexed.get(gap.feature.as_str()) {
            Some(indexed_gap) if **indexed_gap == gap => {}
            Some(_) => diagnostics.push(format!(
                "`{relative}` differs from its `{GAP_PATH}` index row"
            )),
            None => diagnostics.push(format!(
                "`{relative}` feature `{}` is missing from `{GAP_PATH}`",
                gap.feature
            )),
        }
    }
    for feature in indexed.keys() {
        if !seen.contains(*feature) {
            diagnostics.push(format!(
                "`{GAP_PATH}` feature `{feature}` lacks a `{GAP_TOML_DIR}` manifest"
            ));
        }
    }
    Ok(diagnostics)
}

pub(crate) fn read_gap_policy(root: &Path) -> QualityResult<GapPolicy> {
    let path = root.join(GAP_POLICY_PATH);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read gap policy: {err}", path.display()))?;
    let policy = basic_toml::from_str::<GapPolicy>(&text)
        .map_err(|err| format!("{}: invalid gap policy TOML: {err}", path.display()))?;
    if policy.schema != "terlan.lean-proof-gap-policy.v1" {
        return Err(format!(
            "{GAP_POLICY_PATH}: unsupported schema `{}`",
            policy.schema
        ));
    }
    if !(1..=365).contains(&policy.max_blocker_age_days) {
        return Err(format!(
            "{GAP_POLICY_PATH}: max_blocker_age_days must be between 1 and 365"
        ));
    }
    Ok(policy)
}

pub(crate) fn current_utc_date() -> Date {
    OffsetDateTime::now_utc().date()
}

pub(crate) fn validate_gap_lifecycle(
    gaps: &[LeanProofGap],
    policy: &GapPolicy,
    today: Date,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for gap in gaps {
        if !LIFECYCLE_STATUSES.contains(&gap.lifecycle_status.as_str()) {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` has invalid lifecycle status `{}`",
                gap.feature, gap.lifecycle_status
            ));
        }
        if gap.lifecycle_status == "open" {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` remains `open`; triage it before committing the gap manifest",
                gap.feature
            ));
        }
        if !GAP_CATEGORIES.contains(&gap.category.as_str()) {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` has invalid proof-gap category `{}`",
                gap.feature, gap.category
            ));
        }
        if !has_deadline_or_exception(&gap.deadline_or_exception) {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` must use `deadline:<value>` or `exception:<token>`",
                gap.feature
            ));
        }
        match parse_date(&gap.blocker_updated_at) {
            Ok(updated_at) => {
                let age = (today - updated_at).whole_days();
                if age < 0 {
                    diagnostics.push(format!(
                        "`{GAP_PATH}` row `{}` blocker timestamp `{}` is in the future",
                        gap.feature, gap.blocker_updated_at
                    ));
                } else if age > policy.max_blocker_age_days {
                    diagnostics.push(format!(
                        "`{GAP_PATH}` row `{}` blocker is {age} days old, exceeding the {} day TTL",
                        gap.feature, policy.max_blocker_age_days
                    ));
                }
            }
            Err(message) => {
                diagnostics.push(format!(
                    "`{GAP_PATH}` row `{}` has invalid blocker timestamp `{}`: {message}",
                    gap.feature, gap.blocker_updated_at
                ));
            }
        }
        let expected = blocker_hash(
            &gap.feature,
            &gap.category,
            &gap.reason,
            &gap.blocker_updated_at,
        );
        if gap.blocker_hash != expected {
            diagnostics.push(format!(
                "`{GAP_PATH}` row `{}` has stale blocker hash: expected `{expected}`, found `{}`",
                gap.feature, gap.blocker_hash
            ));
        }
    }
    diagnostics
}

pub(crate) fn blocker_hash(
    feature: &str,
    category: &str,
    reason: &str,
    blocker_updated_at: &str,
) -> String {
    let mut hasher = Sha256::new();
    for field in [feature, category, reason, blocker_updated_at] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    let hexadecimal = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hexadecimal}")
}

pub(crate) fn write_gap_metrics(
    root: &Path,
    gaps: &[LeanProofGap],
    policy: &GapPolicy,
    today: Date,
) -> QualityResult<()> {
    let metrics = gaps
        .iter()
        .map(|gap| {
            let updated_at = parse_date(&gap.blocker_updated_at)?;
            Ok(GapMetric {
                feature: gap.feature.clone(),
                lifecycle_status: gap.lifecycle_status.clone(),
                gap_staleness_days: (today - updated_at).whole_days(),
                gap_classification_confidence: classification_confidence(&gap.lifecycle_status),
            })
        })
        .collect::<QualityResult<Vec<_>>>()?;
    let max_staleness = metrics
        .iter()
        .map(|metric| metric.gap_staleness_days)
        .max()
        .unwrap_or(0);
    let confidence = if metrics.is_empty() {
        1.0
    } else {
        metrics
            .iter()
            .map(|metric| metric.gap_classification_confidence)
            .sum::<f64>()
            / metrics.len() as f64
    };

    let path = root.join(GATE_REPORT_PATH);
    let mut report = serde_json::from_str::<Value>(&fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read proof gate report: {err}",
            path.display()
        )
    })?)
    .map_err(|err| format!("{}: invalid proof gate report JSON: {err}", path.display()))?;
    let object = report
        .as_object_mut()
        .ok_or_else(|| format!("{}: proof gate report must be an object", path.display()))?;
    object.insert(
        "proof_gap_metrics".to_string(),
        json!({
            "policy": {
                "max_blocker_age_days": policy.max_blocker_age_days,
            },
            "gap_count": metrics.len(),
            "gap_staleness_days": max_staleness,
            "gap_classification_confidence": confidence,
            "unresolved_open_count": metrics
                .iter()
                .filter(|metric| metric.lifecycle_status == "open")
                .count(),
            "gaps": metrics,
        }),
    );
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&report).map_err(|err| format!(
                "{}: failed to serialize proof gate report: {err}",
                path.display()
            ))?
        ),
    )
    .map_err(|err| {
        format!(
            "{}: failed to write proof gate report: {err}",
            path.display()
        )
    })
}

pub(super) fn parse_date(value: &str) -> QualityResult<Date> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err("expected YYYY-MM-DD".to_string());
    }
    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| "year is not numeric".to_string())?;
    let month = parts[1]
        .parse::<u8>()
        .map_err(|_| "month is not numeric".to_string())?;
    let day = parts[2]
        .parse::<u8>()
        .map_err(|_| "day is not numeric".to_string())?;
    let month = Month::try_from(month).map_err(|_| "month is out of range".to_string())?;
    Date::from_calendar_date(year, month, day).map_err(|_| "date is out of range".to_string())
}

fn classification_confidence(status: &str) -> f64 {
    match status {
        "blocked" | "remediated" | "closed" => 1.0,
        "triaged" => 0.75,
        _ => 0.0,
    }
}

fn has_deadline_or_exception(value: &str) -> bool {
    ["deadline:", "exception:"].iter().any(|prefix| {
        value
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.trim().is_empty())
    })
}

fn validate_lane_exception(path: &str, value: &str, today: Date) -> Vec<String> {
    let Some(exception) = value.strip_prefix("exception:") else {
        return vec![format!(
            "`{path}` must carry an owner-approved `exception:<lane>@YYYY-MM-DD` window"
        )];
    };
    let Some((lane, expiry)) = exception.split_once('@') else {
        return vec![format!(
            "`{path}` exception must use `exception:<lane>@YYYY-MM-DD`"
        )];
    };
    let mut diagnostics = Vec::new();
    if !PROOF_LANES.contains(&lane) {
        diagnostics.push(format!("`{path}` exception names unknown lane `{lane}`"));
    }
    match parse_date(expiry) {
        Ok(expiry) if expiry >= today => {}
        Ok(_) => diagnostics.push(format!("`{path}` exception expired on `{expiry}`")),
        Err(error) => diagnostics.push(format!(
            "`{path}` exception expiry `{expiry}` is invalid: {error}"
        )),
    }
    diagnostics
}

fn gap_file_stem(feature: &str) -> String {
    let mut stem = String::new();
    let mut separator = false;
    for character in feature.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !stem.is_empty() {
                stem.push('_');
            }
            stem.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    stem
}

fn split_list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "lean_proof_gap_test.rs"]
#[cfg(test)]
mod lean_proof_gap_test;
