use std::collections::{BTreeMap, BTreeSet};

use time::Date;

use crate::terlan_quality::lean_proof_track::lean_proof_gap::{parse_date, LeanProofGap};
use crate::terlan_quality::QualityResult;

pub(crate) const TRANSITION_PATH: &str = "docs/compiler/proof_track/lean_proof_gap_transitions.tsv";
const TRANSITION_HEADER: &str =
    "feature\tprevious_status\tnext_status\tchanged_at\tevidence_hash\trationale";
const LIFECYCLE: &[&str] = &["none", "open", "triaged", "blocked", "remediated", "closed"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GapTransition {
    feature: String,
    previous_status: String,
    next_status: String,
    changed_at: String,
    evidence_hash: String,
    rationale: String,
}

pub(crate) fn parse_gap_transitions(text: &str) -> QualityResult<Vec<GapTransition>> {
    let mut lines = text.lines();
    match lines.next() {
        Some(header) if header == TRANSITION_HEADER => {}
        Some(header) => {
            return Err(format!(
                "`{TRANSITION_PATH}` header mismatch: expected `{TRANSITION_HEADER}`, found `{header}`"
            ));
        }
        None => return Err(format!("`{TRANSITION_PATH}` is empty")),
    }

    let mut transitions = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 6 {
            return Err(format!(
                "`{TRANSITION_PATH}` row {} has {} columns, expected 6",
                index + 2,
                columns.len()
            ));
        }
        transitions.push(GapTransition {
            feature: columns[0].to_string(),
            previous_status: columns[1].to_string(),
            next_status: columns[2].to_string(),
            changed_at: columns[3].to_string(),
            evidence_hash: columns[4].to_string(),
            rationale: columns[5].to_string(),
        });
    }
    Ok(transitions)
}

pub(crate) fn validate_gap_transitions(
    gaps: &[LeanProofGap],
    transitions: &[GapTransition],
    today: Date,
) -> Vec<String> {
    let gap_features = gaps
        .iter()
        .map(|gap| gap.feature.as_str())
        .collect::<BTreeSet<_>>();
    let mut by_feature = BTreeMap::<&str, Vec<&GapTransition>>::new();
    let mut diagnostics = Vec::new();

    for transition in transitions {
        if !gap_features.contains(transition.feature.as_str()) {
            diagnostics.push(format!(
                "proof_gap[orphaned-transition]: `{}` has history but no gap row",
                transition.feature
            ));
        }
        if transition.rationale.trim().is_empty() {
            diagnostics.push(format!(
                "proof_gap[missing-transition-rationale]: `{}` has an empty transition rationale",
                transition.feature
            ));
        }
        if !is_sha256(&transition.evidence_hash) {
            diagnostics.push(format!(
                "proof_gap[invalid-transition-evidence]: `{}` transition evidence `{}` is not SHA-256",
                transition.feature, transition.evidence_hash
            ));
        }
        by_feature
            .entry(transition.feature.as_str())
            .or_default()
            .push(transition);
    }

    for gap in gaps {
        let history = by_feature
            .get(gap.feature.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        if history.is_empty() {
            diagnostics.push(format!(
                "proof_gap[missing-transition-history]: `{}` has no lifecycle history",
                gap.feature
            ));
            continue;
        }

        let mut expected_previous = "none";
        let mut previous_date = None;
        for transition in history {
            if transition.previous_status != expected_previous {
                diagnostics.push(format!(
                    "proof_gap[disconnected-transition]: `{}` expected previous status `{}`, found `{}`",
                    gap.feature, expected_previous, transition.previous_status
                ));
            }
            if !is_successor(&transition.previous_status, &transition.next_status) {
                diagnostics.push(format!(
                    "proof_gap[invalid-transition]: `{}` cannot move from `{}` to `{}`",
                    gap.feature, transition.previous_status, transition.next_status
                ));
            }
            match parse_date(&transition.changed_at) {
                Ok(changed_at) => {
                    if changed_at > today {
                        diagnostics.push(format!(
                            "proof_gap[future-transition]: `{}` transition date `{}` is in the future",
                            gap.feature, transition.changed_at
                        ));
                    }
                    if previous_date.is_some_and(|previous| changed_at < previous) {
                        diagnostics.push(format!(
                            "proof_gap[out-of-order-transition]: `{}` transition date `{}` precedes its prior entry",
                            gap.feature, transition.changed_at
                        ));
                    }
                    previous_date = Some(changed_at);
                }
                Err(message) => diagnostics.push(format!(
                    "proof_gap[invalid-transition-date]: `{}` has date `{}`: {message}",
                    gap.feature, transition.changed_at
                )),
            }
            expected_previous = transition.next_status.as_str();
        }

        if expected_previous != gap.lifecycle_status {
            diagnostics.push(format!(
                "proof_gap[transition-state-drift]: `{}` history ends at `{expected_previous}`, manifest is `{}`",
                gap.feature, gap.lifecycle_status
            ));
        }
        if gap.lifecycle_status != "closed"
            && history.last().is_some_and(|transition| {
                transition.evidence_hash.as_str() != gap.blocker_hash.as_str()
            })
        {
            diagnostics.push(format!(
                "proof_gap[transition-evidence-drift]: `{}` latest transition must use blocker hash `{}`",
                gap.feature, gap.blocker_hash
            ));
        }
    }
    diagnostics
}

fn is_successor(previous: &str, next: &str) -> bool {
    LIFECYCLE
        .windows(2)
        .any(|pair| pair[0] == previous && pair[1] == next)
}

fn is_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[cfg(test)]
#[path = "lean_proof_gap_transition_test.rs"]
mod lean_proof_gap_transition_test;
