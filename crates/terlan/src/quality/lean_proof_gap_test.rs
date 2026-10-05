use std::fs;

use super::*;

fn today() -> Date {
    Date::from_calendar_date(2026, Month::July, 16).expect("valid fixture date")
}

fn policy() -> GapPolicy {
    GapPolicy {
        schema: "terlan.lean-proof-gap-policy.v2".to_string(),
        approval_scope: "release".to_string(),
        release_version: "0.0.9".to_string(),
    }
}

fn gap(status: &str, category: &str, deadline: &str) -> LeanProofGap {
    let feature = "typed CoreIR preservation";
    let reason = "Constructors remain unproved.";
    let blocker_updated_at = "2026-07-16";
    LeanProofGap {
        feature: feature.to_string(),
        lifecycle_status: status.to_string(),
        category: category.to_string(),
        reason: reason.to_string(),
        remediation_owner: "compiler".to_string(),
        planned_gate: "core-typing-spec-check".to_string(),
        deadline_or_exception: deadline.to_string(),
        blocker_updated_at: blocker_updated_at.to_string(),
        blocker_hash: blocker_hash(feature, category, reason, blocker_updated_at),
        covered_manifests: vec!["docs/compiler/core.toml".to_string()],
    }
}

#[test]
fn lean_proof_gap_lifecycle_accepts_content_addressed_blocker() {
    assert!(validate_gap_lifecycle(
        &[gap("blocked", "model_gap", "deadline:0.0.7-closeout")],
        &policy(),
        today(),
    )
    .is_empty());
}

#[test]
fn lean_proof_gap_lifecycle_rejects_untriaged_open_gap() {
    let diagnostics = validate_gap_lifecycle(
        &[gap("open", "model_gap", "deadline:0.0.7-closeout")],
        &policy(),
        today(),
    );
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("remains `open`")));
}

#[test]
fn lean_proof_gap_lifecycle_rejects_unknown_status_and_category() {
    let diagnostics = validate_gap_lifecycle(
        &[gap("paused", "unknown", "deadline:later")],
        &policy(),
        today(),
    );
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("invalid lifecycle status `paused`")));
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("invalid proof-gap category `unknown`")));
}

#[test]
fn lean_proof_gap_lifecycle_rejects_missing_deadline_or_exception() {
    let diagnostics =
        validate_gap_lifecycle(&[gap("blocked", "model_gap", "none")], &policy(), today());
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("`deadline:<value>` or `exception:<token>`")));
}

#[test]
fn lean_proof_gap_lifecycle_rejects_stale_blocker_hash_after_reason_change() {
    let mut row = gap("blocked", "model_gap", "deadline:0.0.7-closeout");
    row.reason.push_str(" Calls remain unproved.");

    let diagnostics = validate_gap_lifecycle(&[row], &policy(), today());
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("has stale blocker hash")));
}

#[test]
fn lean_proof_gap_lifecycle_accepts_old_reviews_but_rejects_future_timestamps() {
    let mut stale = gap("blocked", "model_gap", "deadline:0.0.7-closeout");
    stale.blocker_updated_at = "2026-06-15".to_string();
    stale.blocker_hash = blocker_hash(
        &stale.feature,
        &stale.category,
        &stale.reason,
        &stale.blocker_updated_at,
    );
    let mut future = gap("blocked", "model_gap", "deadline:0.0.7-closeout");
    future.blocker_updated_at = "2026-07-17".to_string();
    future.blocker_hash = blocker_hash(
        &future.feature,
        &future.category,
        &future.reason,
        &future.blocker_updated_at,
    );

    assert!(validate_gap_lifecycle(&[stale], &policy(), today()).is_empty());
    let diagnostics = validate_gap_lifecycle(&[future], &policy(), today());
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("in the future")));
}

#[test]
fn lean_proof_gap_lifecycle_preserves_closed_blocker_history() {
    let mut closed = gap("closed", "model_gap", "exception:coreir@2026-07-15");
    closed.blocker_updated_at = "2026-06-15".to_string();
    closed.blocker_hash = blocker_hash(
        &closed.feature,
        &closed.category,
        &closed.reason,
        &closed.blocker_updated_at,
    );

    assert!(validate_gap_lifecycle(&[closed], &policy(), today()).is_empty());
}

#[test]
fn lean_proof_gap_closed_exception_retains_history_after_expiry() {
    assert!(validate_lane_exception(
        "proofs/lean/gaps/closed.toml",
        "exception:coreir@2026-07-15",
        "0.0.9",
        false,
    )
    .is_empty());
}

#[test]
fn lean_proof_gap_metrics_emit_freshness_without_a_published_report() {
    let metrics = gap_metrics(
        &[gap("blocked", "model_gap", "deadline:0.0.7-closeout")],
        &policy(),
        today(),
    )
    .expect("construct metrics");
    assert_eq!(metrics["gap_staleness_days"], 0);
    assert_eq!(metrics["gap_classification_confidence"], 1.0);
    assert_eq!(metrics["gap_count"], 1);
    assert_eq!(metrics["unresolved_open_count"], 0);
    assert_eq!(metrics["gaps"][0]["feature"], "typed CoreIR preservation");
}

#[test]
fn lean_proof_gap_toml_mirror_accepts_owner_approved_release_exception() {
    let root = std::env::temp_dir().join(format!(
        "terlan_lean_gap_toml_{}_{}",
        std::process::id(),
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    fs::create_dir_all(root.join(GAP_TOML_DIR)).expect("gap directory");
    let gap = gap("blocked", "model_gap", "exception:coreir@release-0.0.9");
    fs::write(
        root.join(GAP_TOML_DIR)
            .join("typed_coreir_preservation.toml"),
        format!(
            concat!(
                "schema = \"terlan.lean-proof-gap.v1\"\n",
                "feature = \"{}\"\n",
                "lifecycle_status = \"{}\"\n",
                "proof_gap_category = \"{}\"\n",
                "gap_reason = \"{}\"\n",
                "remediation_owner = \"{}\"\n",
                "planned_gate = \"{}\"\n",
                "deadline_or_exception = \"{}\"\n",
                "exception_approved_by = \"compiler\"\n",
                "blocker_updated_at = \"{}\"\n",
                "blocker_hash = \"{}\"\n",
                "covered_manifests = [\"{}\"]\n"
            ),
            gap.feature,
            gap.lifecycle_status,
            gap.category,
            gap.reason,
            gap.remediation_owner,
            gap.planned_gate,
            gap.deadline_or_exception,
            gap.blocker_updated_at,
            gap.blocker_hash,
            gap.covered_manifests[0],
        ),
    )
    .expect("gap TOML");

    let diagnostics =
        validate_gap_toml_mirror(&root, &[gap], "0.0.9").expect("validate TOML mirror");

    fs::remove_dir_all(root).expect("remove fixture");
    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn lean_proof_gap_toml_mirror_rejects_wrong_approver_and_previous_release_exception() {
    let root = std::env::temp_dir().join(format!(
        "terlan_lean_gap_toml_bad_{}_{}",
        std::process::id(),
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    fs::create_dir_all(root.join(GAP_TOML_DIR)).expect("gap directory");
    let gap = gap("blocked", "model_gap", "exception:coreir@release-0.0.8");
    fs::write(
        root.join(GAP_TOML_DIR)
            .join("typed_coreir_preservation.toml"),
        format!(
            concat!(
                "schema = \"terlan.lean-proof-gap.v1\"\n",
                "feature = \"{}\"\n",
                "lifecycle_status = \"{}\"\n",
                "proof_gap_category = \"{}\"\n",
                "gap_reason = \"{}\"\n",
                "remediation_owner = \"{}\"\n",
                "planned_gate = \"{}\"\n",
                "deadline_or_exception = \"{}\"\n",
                "exception_approved_by = \"vm\"\n",
                "blocker_updated_at = \"{}\"\n",
                "blocker_hash = \"{}\"\n",
                "covered_manifests = [\"{}\"]\n"
            ),
            gap.feature,
            gap.lifecycle_status,
            gap.category,
            gap.reason,
            gap.remediation_owner,
            gap.planned_gate,
            gap.deadline_or_exception,
            gap.blocker_updated_at,
            gap.blocker_hash,
            gap.covered_manifests[0],
        ),
    )
    .expect("gap TOML");

    let diagnostics =
        validate_gap_toml_mirror(&root, &[gap], "0.0.9").expect("validate TOML mirror");

    fs::remove_dir_all(root).expect("remove fixture");
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("approver `vm`")));
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("release `0.0.9` requires owner approval")));
}

#[test]
fn lean_proof_gap_release_approval_is_stable_across_calendar_years() {
    let row = gap("blocked", "model_gap", "exception:coreir@release-0.0.9");
    for year in [2026, 2027, 2036] {
        let date = Date::from_calendar_date(year, Month::December, 31).unwrap();
        assert!(validate_gap_lifecycle(std::slice::from_ref(&row), &policy(), date).is_empty());
        assert!(
            validate_lane_exception("fixture", &row.deadline_or_exception, "0.0.9", true)
                .is_empty()
        );
    }
    assert!(
        !validate_lane_exception("fixture", &row.deadline_or_exception, "0.0.10", true).is_empty()
    );
}

#[test]
fn lean_proof_gap_active_approval_rejects_calendar_unknown_lane_and_empty_version() {
    for approval in [
        "exception:coreir@2099-12-31",
        "exception:unknown@release-0.0.9",
        "exception:coreir@release-",
    ] {
        assert!(
            !validate_lane_exception("fixture", approval, "0.0.9", true).is_empty(),
            "{approval}"
        );
    }
}

#[test]
fn lean_proof_gap_policy_reads_the_source_workspace_version() {
    let root = std::env::temp_dir().join(format!(
        "terlan_gap_release_{}_{}",
        std::process::id(),
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    ));
    fs::create_dir_all(root.join("docs/compiler/proof_track")).unwrap();
    fs::write(
        root.join(GAP_POLICY_PATH),
        "schema = \"terlan.lean-proof-gap-policy.v2\"\napproval_scope = \"release\"\n",
    )
    .unwrap();
    assert!(read_gap_policy(&root).is_err());
    for version in ["0.0.9", "0.0.10"] {
        fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.package]\nversion = \"{version}\"\n"),
        )
        .unwrap();
        assert_eq!(read_gap_policy(&root).unwrap().release_version, version);
    }
    fs::remove_dir_all(root).unwrap();
}
