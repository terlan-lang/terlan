use super::*;
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{parse_gap_manifest, GAP_HEADER};
use time::{Date, Month};

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn gap(status: &str) -> LeanProofGap {
    let text = format!(
        concat!(
            "{header}\ntyped CoreIR preservation\t{status}\tmodel_gap\t",
            "Constructors remain unproved.\tcompiler\tcore-typing-spec-check\t",
            "deadline:0.0.7-closeout\t2026-07-16\t{hash}\t",
            "docs/compiler/type_spec/terlan_core_typing_spec.toml\n"
        ),
        header = GAP_HEADER,
        status = status,
        hash = HASH
    );
    parse_gap_manifest(&text)
        .expect("valid gap fixture")
        .remove(0)
}

fn transition(previous: &str, next: &str, changed_at: &str) -> GapTransition {
    GapTransition {
        feature: "typed CoreIR preservation".to_string(),
        previous_status: previous.to_string(),
        next_status: next.to_string(),
        changed_at: changed_at.to_string(),
        evidence_hash: HASH.to_string(),
        rationale: format!("Move from {previous} to {next}."),
    }
}

fn blocked_history() -> Vec<GapTransition> {
    vec![
        transition("none", "open", "2026-07-16"),
        transition("open", "triaged", "2026-07-16"),
        transition("triaged", "blocked", "2026-07-16"),
    ]
}

fn today() -> Date {
    Date::from_calendar_date(2026, Month::July, 17).expect("valid fixture date")
}

#[test]
fn lean_proof_gap_hygiene_transition_accepts_strict_blocked_chain() {
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &blocked_history(), today());

    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_skipped_lifecycle_state() {
    let history = vec![
        transition("none", "open", "2026-07-16"),
        transition("open", "blocked", "2026-07-16"),
    ];
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &history, today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[invalid-transition]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_disconnected_history() {
    let history = vec![
        transition("none", "open", "2026-07-16"),
        transition("triaged", "blocked", "2026-07-16"),
    ];
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &history, today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[disconnected-transition]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_manifest_state_drift() {
    let history = vec![transition("none", "open", "2026-07-16")];
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &history, today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[transition-state-drift]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_future_date_and_invalid_evidence() {
    let mut history = blocked_history();
    history[2].changed_at = "2026-07-18".to_string();
    history[2].evidence_hash = "sha256:not-a-digest".to_string();
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &history, today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[future-transition]")));
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[invalid-transition-evidence]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_out_of_order_dates() {
    let mut history = blocked_history();
    history[0].changed_at = "2026-07-17".to_string();
    let diagnostics = validate_gap_transitions(&[gap("blocked")], &history, today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[out-of-order-transition]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_orphaned_history() {
    let diagnostics = validate_gap_transitions(&[], &blocked_history(), today());

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[orphaned-transition]")));
}

fn reviewed_gap_and_history() -> (LeanProofGap, Vec<GapTransition>) {
    let mut current = gap("blocked");
    current.blocker_hash = format!("sha256:{}", "b".repeat(64));
    current.blocker_updated_at = "2026-07-17".to_string();
    let mut history = blocked_history();
    let mut review = transition("blocked", "blocked", "2026-07-17");
    review.evidence_hash = current.blocker_hash.clone();
    review.rationale =
        "Review: Arithmetic remains the only modeled constructor family.".to_string();
    history.push(review);
    (current, history)
}

#[test]
fn lean_proof_gap_hygiene_transition_accepts_append_only_blocker_review() {
    let (current, history) = reviewed_gap_and_history();
    let diagnostics = validate_gap_transitions(&[current], &history, today());
    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
    assert_eq!(history[2].evidence_hash, HASH);
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_review_without_new_evidence_or_rationale() {
    for missing_evidence in [true, false] {
        let (mut current, mut history) = reviewed_gap_and_history();
        if missing_evidence {
            current.blocker_hash = HASH.to_string();
            history[3].evidence_hash = HASH.to_string();
        } else {
            history[3].rationale = "Review: ".to_string();
        }
        let diagnostics = validate_gap_transitions(&[current], &history, today());
        assert!(diagnostics
            .iter()
            .any(|item| item.contains("proof_gap[invalid-blocker-review]")));
    }
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_review_date_drift_or_replay() {
    let (mut current, history) = reviewed_gap_and_history();
    current.blocker_updated_at = "2026-07-16".to_string();
    assert!(validate_gap_transitions(&[current], &history, today())
        .iter()
        .any(|item| item.contains("proof_gap[review-date-drift]")));

    let (current, mut history) = reviewed_gap_and_history();
    history[3].changed_at = "2026-07-16".to_string();
    assert!(validate_gap_transitions(&[current], &history, today())
        .iter()
        .any(|item| item.contains("proof_gap[invalid-review-date]")));
}

#[test]
fn lean_proof_gap_hygiene_transition_rejects_reviews_of_nonblocked_states() {
    for status in ["open", "triaged", "remediated", "closed"] {
        let (current, mut history) = reviewed_gap_and_history();
        history[3].previous_status = status.to_string();
        history[3].next_status = status.to_string();
        assert!(validate_gap_transitions(&[current], &history, today())
            .iter()
            .any(|item| item.contains("proof_gap[invalid-transition]")));
    }
}
