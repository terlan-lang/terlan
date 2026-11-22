use super::*;
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{GapPolicy, GAP_HEADER};
use time::{Date, Month};

fn policy() -> GapPolicy {
    toml::from_str("schema = \"terlan.lean-proof-gap-policy.v2\"\napproval_scope = \"release\"\n")
        .expect("valid gap policy")
}

fn gap_with_status(status: &str, updated_at: &str, planned_gate: &str) -> LeanProofGap {
    let feature = "typed CoreIR preservation";
    let category = "model_gap";
    let reason = "Constructors remain unproved.";
    let text = format!(
        concat!(
            "{GAP_HEADER}\n{feature}\t{status}\t{category}\t{reason}\tcompiler\t",
            "{planned_gate}\tdeadline:0.0.7-closeout\t{updated_at}\t",
            "docs/compiler/type_spec/terlan_core_typing_spec.toml\n"
        ),
        GAP_HEADER = GAP_HEADER,
        feature = feature,
        status = status,
        category = category,
        reason = reason,
        planned_gate = planned_gate,
        updated_at = updated_at
    );
    parse_gap_manifest(&text)
        .expect("valid gap manifest")
        .remove(0)
}

fn gap(updated_at: &str, planned_gate: &str) -> LeanProofGap {
    gap_with_status("blocked", updated_at, planned_gate)
}

fn artifact(status: &str) -> ArtifactRow {
    ArtifactRow {
        path: "proofs/lean/Core.lean".to_string(),
        status: status.to_string(),
        theorem_scope: "CoreIR".to_string(),
        targeted_manifests: vec!["docs/compiler/type_spec/terlan_core_typing_spec.toml".to_string()],
        expected_exit: 0,
        stderr_class: "none".to_string(),
        replay_metadata: "proofs/lean/artifacts/core.json".to_string(),
        remediation_plan: "none".to_string(),
    }
}

fn closure(feature: &str, artifact_path: &str) -> ClosureNote {
    ClosureNote {
        feature: feature.to_string(),
        artifact_path: artifact_path.to_string(),
        rationale: "The executable proof now covers the complete feature.".to_string(),
    }
}

fn inventory(status: &str, source_contract: &str) -> InventoryRow {
    parse_inventory(&format!(
        concat!(
            "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes\n",
            "proofs/lean/Core.lean\t{status}\t{source_contract}\t0.0.7\t",
            "lean-proof-track-check\ttest fixture\n"
        ),
        status = status,
        source_contract = source_contract
    ))
    .expect("valid inventory")
    .remove(0)
}

fn today() -> Date {
    Date::from_calendar_date(2026, Month::July, 17).expect("valid fixture date")
}

#[test]
fn lean_proof_gap_hygiene_rejects_duplicate_current_feature_coverage() {
    let diagnostics = validate_gap_hygiene(
        &[gap("2026-07-16", "core-typing-spec-check")],
        &[inventory("current", "typed CoreIR preservation")],
        &[],
        &[],
        &BTreeSet::from(["core-typing-spec-check".to_string()]),
        &policy(),
        today(),
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[duplicate-coverage]")));
}

#[test]
fn lean_proof_gap_hygiene_allows_narrow_current_seed_coverage() {
    let diagnostics = validate_gap_hygiene(
        &[gap("2026-07-16", "core-typing-spec-check")],
        &[inventory("current", "CoreIR integer arithmetic seed")],
        &[],
        &[],
        &BTreeSet::from(["core-typing-spec-check".to_string()]),
        &policy(),
        today(),
    );

    assert!(!diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[duplicate-coverage]")));
}

#[test]
fn lean_proof_gap_hygiene_rejects_non_executable_follow_up_gate() {
    let diagnostics = validate_gap_hygiene(
        &[gap("2026-07-16", "missing-proof-check")],
        &[],
        &[],
        &[],
        &BTreeSet::new(),
        &policy(),
        today(),
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("is not defined by Make targets")));
}

#[test]
fn lean_proof_gap_hygiene_does_not_expire_unchanged_blocker_by_age() {
    let diagnostics = validate_gap_hygiene(
        &[gap("2026-06-16", "core-typing-spec-check")],
        &[],
        &[],
        &[],
        &BTreeSet::from(["core-typing-spec-check".to_string()]),
        &policy(),
        today(),
    );

    let fresh_diagnostics = validate_gap_hygiene(
        &[gap("2026-07-16", "core-typing-spec-check")],
        &[],
        &[],
        &[],
        &BTreeSet::from(["core-typing-spec-check".to_string()]),
        &policy(),
        today(),
    );
    assert_eq!(diagnostics, fresh_diagnostics);
}

#[test]
fn lean_proof_gap_hygiene_rejects_closed_gap_without_changelog_note() {
    let diagnostics = validate_gap_closures(
        &[gap_with_status(
            "closed",
            "2026-07-16",
            "core-typing-spec-check",
        )],
        &[artifact("current")],
        &[],
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[missing-closure]")));
}

#[test]
fn lean_proof_gap_hygiene_rejects_orphaned_and_fabricated_closure_note() {
    let diagnostics = validate_gap_closures(
        &[gap("2026-07-16", "core-typing-spec-check")],
        &[artifact("current")],
        &[closure(
            "typed CoreIR preservation",
            "proofs/lean/Missing.lean",
        )],
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[orphaned-closure]")));
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[invalid-closure-artifact]")));
}

#[test]
fn lean_proof_gap_hygiene_rejects_duplicate_closure_notes() {
    let note = closure("typed CoreIR preservation", "proofs/lean/Core.lean");
    let diagnostics = validate_gap_closures(
        &[gap_with_status(
            "closed",
            "2026-07-16",
            "core-typing-spec-check",
        )],
        &[artifact("current")],
        &[note.clone(), note],
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[duplicate-closure]")));
}

#[test]
fn lean_proof_gap_hygiene_accepts_closed_gap_with_current_artifact_note() {
    let notes = parse_closure_notes(concat!(
        "## Proof-gap closure changelog\n",
        "- Proof-gap closure: `typed CoreIR preservation` restored by `proofs/lean/Core.lean`: ",
        "The complete proof is executable.\n"
    ))
    .expect("canonical closure note");
    let diagnostics = validate_gap_closures(
        &[gap_with_status(
            "closed",
            "2026-07-16",
            "core-typing-spec-check",
        )],
        &[artifact("current")],
        &notes,
    );

    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn lean_proof_gap_hygiene_rejects_unrelated_current_closure_artifact() {
    let mut unrelated = artifact("current");
    unrelated.targeted_manifests = vec!["docs/compiler/unrelated.toml".to_string()];
    let diagnostics = validate_gap_closures(
        &[gap_with_status(
            "closed",
            "2026-07-16",
            "core-typing-spec-check",
        )],
        &[unrelated],
        &[closure(
            "typed CoreIR preservation",
            "proofs/lean/Core.lean",
        )],
    );

    assert!(diagnostics
        .iter()
        .any(|item| item.contains("proof_gap[unrelated-closure-artifact]")));
}

#[test]
fn lean_proof_gap_hygiene_rejects_malformed_closure_note_syntax() {
    let error = parse_closure_notes(concat!(
        "## Proof-gap closure changelog\n",
        "- Proof-gap closure: `typed CoreIR preservation` without artifact\n"
    ))
    .expect_err("malformed closure note should fail");

    assert!(error.contains("malformed proof-gap closure feature"));
}

#[test]
fn lean_proof_gap_hygiene_rejects_empty_closure_rationale() {
    let error = parse_closure_notes(concat!(
        "## Proof-gap closure changelog\n",
        "- Proof-gap closure: `typed CoreIR preservation` restored by `proofs/lean/Core.lean`: \n"
    ))
    .expect_err("empty closure rationale should fail");

    assert!(error.contains("closure fields must be non-empty"));
}
