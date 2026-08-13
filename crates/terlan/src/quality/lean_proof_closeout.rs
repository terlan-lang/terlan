use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::QualityResult;

const GATE_REPORT: &str = "build/artifacts/lean-proof-gate.json";
const LANE_REPORT: &str = "build/artifacts/lean-proof-lanes.json";
const SMOKE_REPORT: &str = "build/artifacts/lean-proof-smoke.json";
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
    "parser",
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
const EXPECTED_LANES: &[&str] = &[
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
/// Data describing lean proof closeout summary.
pub struct LeanProofCloseoutSummary {
    pub family_count: usize,
    pub lane_count: usize,
    pub baseline_count: usize,
    pub baseline_hash: String,
    pub gate_report: PathBuf,
}

#[derive(Debug, Deserialize)]
struct GateReport {
    families: Vec<FamilyStatus>,
    lane_matrix_checksum: String,
    lane_checksums: BTreeMap<String, String>,
    proof_gap_metrics: ProofGapMetrics,
}

#[derive(Debug, Deserialize)]
struct ProofGapMetrics {
    unresolved_open_count: usize,
}

#[derive(Debug, Deserialize)]
struct LaneReport {
    schema: String,
    lane_matrix_checksum: String,
    lanes: Vec<LaneStatus>,
}

#[derive(Debug, Deserialize)]
struct LaneStatus {
    lane: String,
    severity: String,
    status: String,
    coverage_status: String,
    duration_ms: u64,
    duration_tolerance_ms: u64,
    number_of_families: usize,
    failed_families: Vec<String>,
    gap_count: usize,
    nondeterministic_count: usize,
    reproducibility_failures: usize,
    smoke_health_score: u8,
    smoke_policy_minimum: u8,
    blockers: Vec<String>,
    checksum: String,
}

#[derive(Debug, Deserialize)]
struct SmokeReport {
    schema: String,
    policy_minimum: u8,
    compatibility_status: String,
    lane_health: BTreeMap<String, u8>,
    blockers: Vec<serde_json::Value>,
    results: Vec<SmokeStatus>,
}

#[derive(Debug, Deserialize)]
struct SmokeStatus {
    smoke_id: String,
    proof_status: String,
    runtime_status: String,
    compatibility_status: String,
    health_score: u8,
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

/// Runs lean proof closeout.
pub fn run_lean_proof_closeout(root: &Path) -> QualityResult<LeanProofCloseoutSummary> {
    let mut diagnostics = validate_foundation(root);
    let gate_text = read(root, GATE_REPORT)?;
    let lane_text = read(root, LANE_REPORT)?;
    let smoke_text = read(root, SMOKE_REPORT)?;
    let baseline_text = read(root, BASELINE)?;
    let gate = parse_gate_report(&gate_text)?;
    let lanes = parse_lane_report(&lane_text)?;
    let smoke = parse_smoke_report(&smoke_text)?;
    let baseline = parse_baseline(&baseline_text)?;
    diagnostics.extend(validate_family_schema(&gate.families));
    diagnostics.extend(validate_lanes(&gate, &lanes));
    diagnostics.extend(validate_smoke(&lanes, &smoke));
    if gate.proof_gap_metrics.unresolved_open_count != 0 {
        diagnostics.push(format!(
            "error[lean_proof_closeout_open_gap]: {} unresolved open proof gaps remain",
            gate.proof_gap_metrics.unresolved_open_count
        ));
    }
    diagnostics.extend(validate_baseline(&baseline));
    diagnostics.extend(validate_closeout(&gate.families, &baseline));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }
    Ok(LeanProofCloseoutSummary {
        family_count: gate.families.len(),
        lane_count: lanes.lanes.len(),
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

fn parse_lane_report(text: &str) -> QualityResult<LaneReport> {
    serde_json::from_str(text)
        .map_err(|err| format!("error[lean_proof_closeout_lanes]: invalid lane JSON: {err}"))
}

fn parse_smoke_report(text: &str) -> QualityResult<SmokeReport> {
    serde_json::from_str(text)
        .map_err(|err| format!("error[lean_proof_closeout_smoke]: invalid smoke JSON: {err}"))
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

fn validate_lanes(gate: &GateReport, report: &LaneReport) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if report.schema != "terlan.lean-proof-lanes.v1" {
        diagnostics.push(format!(
            "error[lean_proof_closeout_lanes]: unsupported lane schema `{}`",
            report.schema
        ));
    }
    if !is_sha256(&report.lane_matrix_checksum)
        || gate.lane_matrix_checksum != report.lane_matrix_checksum
    {
        diagnostics.push(
            "error[lean_proof_closeout_lane_checksum]: lane matrix checksum is missing or differs between reports"
                .to_string(),
        );
    }
    let names = report
        .lanes
        .iter()
        .map(|lane| lane.lane.as_str())
        .collect::<Vec<_>>();
    if names != EXPECTED_LANES {
        diagnostics.push(format!(
            "error[lean_proof_closeout_lanes]: expected ordered lanes `{}`, found `{}`",
            EXPECTED_LANES.join(","),
            names.join(",")
        ));
    }
    if gate.lane_checksums.len() != EXPECTED_LANES.len() {
        diagnostics.push(format!(
            "error[lean_proof_closeout_lane_checksum]: expected {} gate checksums, found {}",
            EXPECTED_LANES.len(),
            gate.lane_checksums.len()
        ));
    }
    for lane in &report.lanes {
        if lane.severity != "hard" || lane.status != "pass" {
            diagnostics.push(format!(
                "error[lean_proof_closeout_lane_status]: lane `{}` is severity `{}` status `{}`",
                lane.lane, lane.severity, lane.status
            ));
        }
        if !lane.blockers.is_empty() {
            diagnostics.push(format!(
                "error[lean_proof_closeout_lane_blocker]: lane `{}` has blockers `{}`",
                lane.lane,
                lane.blockers.join(",")
            ));
        }
        if !lane.failed_families.is_empty()
            || lane.nondeterministic_count != 0
            || lane.reproducibility_failures != 0
        {
            diagnostics.push(format!(
                "error[lean_proof_closeout_lane_evidence]: lane `{}` has failed={}, nondeterministic={}, reproducibility_failures={}",
                lane.lane,
                lane.failed_families.len(),
                lane.nondeterministic_count,
                lane.reproducibility_failures
            ));
        }
        if lane.duration_ms == 0 || lane.duration_tolerance_ms == 0 {
            diagnostics.push(format!(
                "error[lean_proof_closeout_lane_duration]: lane `{}` lacks a positive duration and tolerance",
                lane.lane
            ));
        }
        if lane.smoke_policy_minimum != 100 || lane.smoke_health_score < lane.smoke_policy_minimum {
            diagnostics.push(format!(
                "error[lean_proof_closeout_smoke]: lane `{}` smoke health {} is below policy {}",
                lane.lane, lane.smoke_health_score, lane.smoke_policy_minimum
            ));
        }
        match lane.coverage_status.as_str() {
            "executable_current" if lane.number_of_families == 0 => diagnostics.push(format!(
                "error[lean_proof_closeout_lane_evidence]: executable lane `{}` has no proof families",
                lane.lane
            )),
            "accepted_gap" if lane.number_of_families != 0 || lane.gap_count == 0 => {
                diagnostics.push(format!(
                    "error[lean_proof_closeout_lane_evidence]: accepted-gap lane `{}` has inconsistent family/gap counts",
                    lane.lane
                ));
            }
            "executable_current" | "accepted_gap" => {}
            other => diagnostics.push(format!(
                "error[lean_proof_closeout_lane_evidence]: lane `{}` has unknown coverage `{other}`",
                lane.lane
            )),
        }
        match gate.lane_checksums.get(&lane.lane) {
            Some(checksum) if is_sha256(checksum) && checksum == &lane.checksum => {}
            _ => diagnostics.push(format!(
                "error[lean_proof_closeout_lane_checksum]: lane `{}` lacks a matching gate checksum",
                lane.lane
            )),
        }
    }
    diagnostics
}

fn validate_smoke(lanes: &LaneReport, smoke: &SmokeReport) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if smoke.schema != "terlan.lean-proof-smoke.v1"
        || smoke.compatibility_status != "pass"
        || smoke.policy_minimum != 100
    {
        diagnostics.push(
            "error[lean_proof_closeout_smoke]: smoke schema, compatibility, or policy is not release-clean"
                .to_string(),
        );
    }
    if !smoke.blockers.is_empty() {
        diagnostics.push(format!(
            "error[lean_proof_closeout_smoke]: {} proof/runtime blockers remain",
            smoke.blockers.len()
        ));
    }
    let expected = EXPECTED_LANES
        .iter()
        .map(|lane| (*lane).to_string())
        .collect::<BTreeSet<_>>();
    let actual = smoke.lane_health.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        diagnostics.push(
            "error[lean_proof_closeout_smoke]: smoke report does not score every ordered lane"
                .to_string(),
        );
    }
    for lane in &lanes.lanes {
        match smoke.lane_health.get(&lane.lane) {
            Some(score)
                if *score >= smoke.policy_minimum && *score == lane.smoke_health_score => {}
            _ => diagnostics.push(format!(
                "error[lean_proof_closeout_smoke]: lane `{}` smoke evidence differs between reports",
                lane.lane
            )),
        }
    }
    let mut ids = BTreeSet::new();
    if smoke.results.is_empty() {
        diagnostics.push(
            "error[lean_proof_closeout_smoke]: at least one semantic smoke is required".to_string(),
        );
    }
    for result in &smoke.results {
        if !ids.insert(result.smoke_id.as_str())
            || result.proof_status != "pass"
            || result.runtime_status != "pass"
            || result.compatibility_status != "stable"
            || result.health_score < smoke.policy_minimum
        {
            diagnostics.push(format!(
                "error[lean_proof_closeout_smoke]: smoke `{}` is duplicate, divergent, or below policy",
                result.smoke_id
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
        if row.expected_status == "current" && confirmed_hashes(&row.last_confirmed_hash).is_none()
        {
            diagnostics.push(format!(
                "error[lean_proof_closeout_baseline]: current class `{}` lacks a sorted, unique confirmed SHA-256 hash set",
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
                    && confirmed_hashes(&row.last_confirmed_hash).is_some_and(|hashes| {
                        hashes.contains(&family.last_executed_digest.as_str())
                    }) => {}
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

/// Parses a canonical baseline digest set, rejecting malformed, duplicate, or unsorted hashes.
fn confirmed_hashes(value: &str) -> Option<Vec<&str>> {
    let hashes = value.split(';').collect::<Vec<_>>();
    if hashes.is_empty()
        || hashes.iter().any(|hash| !is_sha256(hash))
        || hashes.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return None;
    }
    Some(hashes)
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
#[cfg(test)]
mod lean_proof_closeout_test;
