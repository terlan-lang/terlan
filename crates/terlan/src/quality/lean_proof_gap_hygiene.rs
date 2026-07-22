use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::terlan_quality::lean_proof_track::gap_transition::{
    parse_gap_transitions, validate_gap_transitions, TRANSITION_PATH,
};
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{
    current_utc_date, parse_gap_manifest, read_gap_policy, validate_gap_lifecycle, LeanProofGap,
    GAP_PATH,
};
use crate::terlan_quality::lean_proof_track::{
    collect_make_check_targets, parse_artifacts, parse_inventory, validate_gaps, ArtifactRow,
    InventoryRow, ARTIFACT_PATH, INVENTORY_PATH, TRACK_DOC,
};
use crate::terlan_quality::QualityResult;

/// Summary produced by the dedicated Lean proof-gap hygiene gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LeanProofGapHygieneSummary {
    pub gap_count: usize,
    pub current_proof_feature_count: usize,
    pub follow_up_gate_count: usize,
    pub closure_note_count: usize,
    pub transition_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClosureNote {
    feature: String,
    artifact_hash: String,
    rationale: String,
}

/// Rejects stale gaps, non-executable plans, and duplicate current coverage.
pub(crate) fn run_lean_proof_gap_hygiene(root: &Path) -> QualityResult<LeanProofGapHygieneSummary> {
    let gap_text = read_text(root, GAP_PATH)?;
    let inventory_text = read_text(root, INVENTORY_PATH)?;
    let artifact_text = read_text(root, ARTIFACT_PATH)?;
    let transition_text = read_text(root, TRANSITION_PATH)?;
    let track_doc = read_text(root, TRACK_DOC)?;
    let gaps = parse_gap_manifest(&gap_text)?;
    let inventory = parse_inventory(&inventory_text)?;
    let artifacts = parse_artifacts(&artifact_text)?;
    let transitions = parse_gap_transitions(&transition_text)?;
    let closure_notes = parse_closure_notes(&track_doc)?;
    let policy = read_gap_policy(root)?;
    let make_check_targets = collect_make_check_targets(root)?;
    let mut diagnostics = validate_gap_hygiene(
        &gaps,
        &inventory,
        &artifacts,
        &closure_notes,
        &make_check_targets,
        &policy,
        current_utc_date(),
    );
    diagnostics.extend(validate_gap_transitions(
        &gaps,
        &transitions,
        current_utc_date(),
    ));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(LeanProofGapHygieneSummary {
        gap_count: gaps.len(),
        current_proof_feature_count: current_proof_features(&inventory).len(),
        follow_up_gate_count: gaps
            .iter()
            .map(|gap| gap.planned_gate.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        closure_note_count: closure_notes.len(),
        transition_count: transitions.len(),
    })
}

fn validate_gap_hygiene(
    gaps: &[LeanProofGap],
    inventory: &[InventoryRow],
    artifacts: &[ArtifactRow],
    closure_notes: &[ClosureNote],
    make_check_targets: &BTreeSet<String>,
    policy: &crate::terlan_quality::lean_proof_track::lean_proof_gap::GapPolicy,
    today: time::Date,
) -> Vec<String> {
    let mut diagnostics = validate_gaps(gaps, make_check_targets);
    diagnostics.extend(validate_gap_lifecycle(gaps, policy, today));

    let current_features = current_proof_features(inventory);
    for gap in gaps {
        if current_features.contains(gap.feature.as_str()) {
            diagnostics.push(format!(
                concat!(
                    "proof_gap[duplicate-coverage]: gap `{}` duplicates a current proof feature; ",
                    "narrow the proof inventory scope or close the gap"
                ),
                gap.feature
            ));
        }
    }
    diagnostics.extend(validate_gap_closures(gaps, artifacts, closure_notes));
    diagnostics
}

fn parse_closure_notes(doc: &str) -> QualityResult<Vec<ClosureNote>> {
    const PREFIX: &str = "- Proof-gap closure: `";
    let mut notes = Vec::new();
    for (index, line) in doc.lines().enumerate() {
        let Some(rest) = line.strip_prefix(PREFIX) else {
            continue;
        };
        let Some((feature, rest)) = rest.split_once("` restored by `") else {
            return Err(format!(
                "{TRACK_DOC}:{}: malformed proof-gap closure feature",
                index + 1
            ));
        };
        let Some((artifact_hash, rationale)) = rest.split_once("`: ") else {
            return Err(format!(
                "{TRACK_DOC}:{}: malformed proof-gap closure artifact or rationale",
                index + 1
            ));
        };
        if feature.trim().is_empty()
            || artifact_hash.trim().is_empty()
            || rationale.trim().is_empty()
        {
            return Err(format!(
                "{TRACK_DOC}:{}: proof-gap closure fields must be non-empty",
                index + 1
            ));
        }
        notes.push(ClosureNote {
            feature: feature.to_string(),
            artifact_hash: artifact_hash.to_string(),
            rationale: rationale.to_string(),
        });
    }
    Ok(notes)
}

fn validate_gap_closures(
    gaps: &[LeanProofGap],
    artifacts: &[ArtifactRow],
    notes: &[ClosureNote],
) -> Vec<String> {
    let closed_features = gaps
        .iter()
        .filter(|gap| gap.lifecycle_status == "closed")
        .map(|gap| gap.feature.as_str())
        .collect::<BTreeSet<_>>();
    let current_hashes = artifacts
        .iter()
        .filter(|artifact| artifact.status == "current")
        .map(|artifact| artifact.proof_digest.as_str())
        .collect::<BTreeSet<_>>();
    let mut diagnostics = Vec::new();
    let mut noted_features = BTreeSet::new();

    for note in notes {
        if !noted_features.insert(note.feature.as_str()) {
            diagnostics.push(format!(
                "proof_gap[duplicate-closure]: feature `{}` has more than one closure note",
                note.feature
            ));
        }
        if !closed_features.contains(note.feature.as_str()) {
            diagnostics.push(format!(
                "proof_gap[orphaned-closure]: feature `{}` has a closure note but is not `closed`",
                note.feature
            ));
        }
        if !current_hashes.contains(note.artifact_hash.as_str()) {
            diagnostics.push(format!(
                concat!(
                    "proof_gap[invalid-closure-artifact]: feature `{}` references `{}`, ",
                    "which is not a current proof artifact digest"
                ),
                note.feature, note.artifact_hash
            ));
        }
        if note.rationale.trim().is_empty() {
            diagnostics.push(format!(
                "proof_gap[missing-closure-rationale]: feature `{}` has an empty closure rationale",
                note.feature
            ));
        }
    }
    for feature in closed_features {
        if !noted_features.contains(feature) {
            diagnostics.push(format!(
                "proof_gap[missing-closure]: closed feature `{feature}` requires a `{TRACK_DOC}` closure note"
            ));
        }
    }
    diagnostics
}

fn current_proof_features(inventory: &[InventoryRow]) -> BTreeSet<&str> {
    inventory
        .iter()
        .filter(|row| row.status == "current")
        .map(|row| row.source_contract.as_str())
        .collect()
}

fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|err| format!("failed to read `{relative}`: {err}"))
}

fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("Lean proof-gap hygiene check failed:\n");
    for diagnostic in diagnostics {
        message.push_str("- ");
        message.push_str(diagnostic);
        message.push('\n');
    }
    message
}

#[cfg(test)]
#[path = "lean_proof_gap_hygiene_test.rs"]
mod lean_proof_gap_hygiene_test;
