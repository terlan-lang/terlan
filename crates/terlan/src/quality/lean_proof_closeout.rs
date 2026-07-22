use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::QualityResult;

const GATE_REPORT: &str = "build/artifacts/lean-proof-gate.json";
const BASELINE: &str = "build/artifacts/lean-proof-baseline.tsv";
const TOOLCHAIN: &str = "proofs/lean/lean-toolchain";
const LAKE_MANIFEST: &str = "proofs/lean/lake-manifest.json";
const EXPECTED_CLASSES: &[&str] = &[
    "coreir",
    "lowering",
    "rejection",
    "runtime",
    "vm",
    "native-boundary",
    "wasm",
    "aeneas-bridge",
];
const VALID_STATUSES: &[&str] = &[
    "current",
    "stale",
    "incomplete",
    "nondeterministic",
    "delete-candidate",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanProofCloseoutSummary {
    pub family_count: usize,
    pub baseline_count: usize,
    pub baseline_hash: String,
    pub gate_report: PathBuf,
}

#[derive(Debug, Deserialize)]
struct GateReport {
    families: Vec<FamilyStatus>,
}

#[derive(Debug, Deserialize)]
struct FamilyStatus {
    family: String,
    feature_class: String,
    theorem_identity: Vec<String>,
    proof_status: String,
    last_executed_digest: String,
    reproducibility_verdict: String,
    blockers: Vec<String>,
    remediation_gates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BaselineRow {
    feature_class: String,
    expected_status: String,
    last_confirmed_hash: String,
}

pub fn run_lean_proof_closeout(root: &Path) -> QualityResult<LeanProofCloseoutSummary> {
    let mut diagnostics = validate_foundation(root);
    let gate_text = read(root, GATE_REPORT)?;
    let baseline_text = read(root, BASELINE)?;
    let gate = parse_gate_report(&gate_text)?;
    let baseline = parse_baseline(&baseline_text)?;
    diagnostics.extend(validate_family_schema(&gate.families));
    diagnostics.extend(validate_baseline(&baseline));
    diagnostics.extend(validate_closeout(&gate.families, &baseline));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(LeanProofCloseoutSummary {
        family_count: gate.families.len(),
        baseline_count: baseline.len(),
        baseline_hash: sha256_text(&baseline_text),
        gate_report: root.join(GATE_REPORT),
    })
}

fn validate_foundation(root: &Path) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if !root.join("proofs/lean/Terlan").is_dir() {
        diagnostics.push(
            "error[lean_proof_closeout_missing_tree]: proofs/lean/Terlan is missing".to_string(),
        );
    }
    match fs::read_to_string(root.join(TOOLCHAIN)) {
        Ok(channel) if channel.trim() == "leanprover/lean4:v4.31.0" => {}
        Ok(channel) => diagnostics.push(format!(
            "error[lean_proof_closeout_toolchain]: expected leanprover/lean4:v4.31.0, found {}",
            channel.trim()
        )),
        Err(err) => diagnostics.push(format!(
            "error[lean_proof_closeout_toolchain]: failed to read {TOOLCHAIN}: {err}"
        )),
    }
    match fs::read_to_string(root.join(LAKE_MANIFEST)) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(value) if value["packages"].as_array().is_some_and(Vec::is_empty) => {}
            Ok(_) => diagnostics.push(
                "error[lean_proof_closeout_lockfile]: Lake lockfile must contain an explicit empty packages array"
                    .to_string(),
            ),
            Err(err) => diagnostics.push(format!(
                "error[lean_proof_closeout_lockfile]: invalid Lake lockfile: {err}"
            )),
        },
        Err(err) => diagnostics.push(format!(
            "error[lean_proof_closeout_lockfile]: failed to read {LAKE_MANIFEST}: {err}"
        )),
    }
    diagnostics
}

fn parse_gate_report(text: &str) -> QualityResult<GateReport> {
    serde_json::from_str(text)
        .map_err(|err| format!("error[lean_proof_closeout_report]: invalid gate JSON: {err}"))
}

fn parse_baseline(text: &str) -> QualityResult<Vec<BaselineRow>> {
    let mut lines = text.lines();
    let expected = "feature_class\texpected_status\tlast_confirmed_hash";
    if lines.next() != Some(expected) {
        return Err(format!(
            "error[lean_proof_closeout_baseline]: expected header `{expected}`"
        ));
    }
    lines
        .enumerate()
        .map(|(index, line)| {
            let columns = line.split('\t').collect::<Vec<_>>();
            if columns.len() != 3 {
                return Err(format!(
                    "error[lean_proof_closeout_baseline]: row {} has {} columns, expected 3",
                    index + 2,
                    columns.len()
                ));
            }
            Ok(BaselineRow {
                feature_class: columns[0].to_string(),
                expected_status: columns[1].to_string(),
                last_confirmed_hash: columns[2].to_string(),
            })
        })
        .collect()
}

fn validate_family_schema(families: &[FamilyStatus]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut names = BTreeSet::new();
    for family in families {
        if !names.insert(family.family.as_str()) {
            diagnostics.push(format!(
                "error[lean_proof_closeout_report]: duplicate family `{}`",
                family.family
            ));
        }
        if family.family.trim().is_empty()
            || family.feature_class.trim().is_empty()
            || family.theorem_identity.is_empty()
            || family.last_executed_digest.trim().is_empty()
        {
            diagnostics.push(
                "error[lean_proof_closeout_report]: family records require identity, class, theorem, and digest"
                    .to_string(),
            );
        }
        if !VALID_STATUSES.contains(&family.proof_status.as_str()) {
            diagnostics.push(format!(
                "error[lean_proof_closeout_status]: family `{}` has invalid status `{}`",
                family.family, family.proof_status
            ));
        }
        if family.proof_status != "current" && family.remediation_gates.is_empty() {
            diagnostics.push(format!(
                "error[lean_proof_closeout_remediation]: family `{}` status `{}` requires a remediation gate",
                family.family, family.proof_status
            ));
        }
    }
    diagnostics
}

fn validate_baseline(rows: &[BaselineRow]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let classes = rows
        .iter()
        .map(|row| row.feature_class.as_str())
        .collect::<Vec<_>>();
    if classes != EXPECTED_CLASSES {
        diagnostics.push(format!(
            "error[lean_proof_closeout_baseline]: expected ordered classes `{}`, found `{}`",
            EXPECTED_CLASSES.join(","),
            classes.join(",")
        ));
    }
    for row in rows {
        if !VALID_STATUSES.contains(&row.expected_status.as_str()) {
            diagnostics.push(format!(
                "error[lean_proof_closeout_baseline]: class `{}` has invalid status `{}`",
                row.feature_class, row.expected_status
            ));
        }
        if row.expected_status == "current" && !is_sha256(&row.last_confirmed_hash) {
            diagnostics.push(format!(
                "error[lean_proof_closeout_baseline]: current class `{}` lacks a confirmed SHA-256 hash",
                row.feature_class
            ));
        }
    }
    diagnostics
}

fn validate_closeout(families: &[FamilyStatus], baseline: &[BaselineRow]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let baseline = baseline
        .iter()
        .map(|row| (row.feature_class.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    if families.is_empty() {
        diagnostics.push(
            "error[lean_proof_closeout_report]: at least one proof family is required".to_string(),
        );
    }
    for family in families {
        if family.proof_status != "current" {
            diagnostics.push(format!(
                "error[lean_proof_closeout_status]: family `{}` is `{}` instead of current",
                family.family, family.proof_status
            ));
        }
        if family.reproducibility_verdict != "pass" {
            diagnostics.push(format!(
                "error[lean_proof_closeout_reproducibility]: family `{}` verdict is `{}`",
                family.family, family.reproducibility_verdict
            ));
        }
        if !family.blockers.is_empty() {
            diagnostics.push(format!(
                "error[lean_proof_closeout_blocker]: family `{}` has blockers `{}`",
                family.family,
                family.blockers.join(",")
            ));
        }
        match baseline.get(family.feature_class.as_str()) {
            Some(row)
                if row.expected_status == "current"
                    && row.last_confirmed_hash == family.last_executed_digest => {}
            _ => diagnostics.push(format!(
                "error[lean_proof_closeout_baseline]: family `{}` is missing a matching current baseline",
                family.family
            )),
        }
    }
    diagnostics
}

fn read(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("error[lean_proof_closeout_missing_artifact]: {relative}: {err}"))
}

fn is_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn sha256_text(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let hexadecimal = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hexadecimal}")
}

fn render_failure(diagnostics: &[String]) -> String {
    diagnostics.join("\n")
}

#[cfg(test)]
#[path = "lean_proof_closeout_test.rs"]
mod lean_proof_closeout_test;
