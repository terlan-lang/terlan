//! Regression coverage for authenticated hosted phase inventories.

use serde_json::{json, Value};

/// The canonical suite owns one completion per selected Terlan harness. Its
/// phase count therefore grows with the suite and must not be constrained by
/// an unrelated fixed ceiling after the bounded report reader has admitted it.
#[test]
fn hosted_reader_accepts_all_completed_phases_in_a_bounded_report() {
    let phases = (1_u64..=282)
        .map(|child_pid| json!({"outcome":"passed", "child_pid":child_pid}))
        .collect::<Vec<_>>();
    let report = json!({"phases":phases});

    assert_eq!(super::passed_phases(&report).unwrap().len(), 282);
}

/// Missing completion evidence remains invalid after removing the stale count
/// ceiling.
#[test]
fn hosted_reader_rejects_empty_phase_inventory() {
    let error = super::passed_phases(&json!({"phases":[]}))
        .expect_err("empty hosted phase inventory must fail");

    assert_eq!(error.detail, "missing completed phases");
}

/// Every dynamically sized inventory still requires a successful observed
/// child completion.
#[test]
fn hosted_reader_rejects_failed_or_unobserved_phase() {
    for phase in [
        json!({"outcome":"failed", "child_pid":1}),
        json!({"outcome":"passed", "child_pid":0}),
        Value::Null,
    ] {
        let error = super::passed_phases(&json!({"phases":[phase]}))
            .expect_err("invalid hosted phase must fail");
        assert_eq!(
            error.detail,
            "hosted phase lacks successful observed child completion"
        );
    }
}
