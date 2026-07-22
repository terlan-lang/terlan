use std::path::Path;

use super::*;

/// Verifies deleted Erlang/BEAM backend paths are no longer accepted.
///
/// Inputs:
/// - Representative path from the deleted Erlang backend tree.
///
/// Output:
/// - Test passes when the deleted path does not resolve to a classification.
///
/// Transformation:
/// - Keeps the old backend path from becoming accepted migration debt again.
#[test]
fn classification_for_path_rejects_deleted_backend_paths() {
    let path = "crates/terlan/src/backends/erlang/emit/core.rs";
    assert!(
        classification_for_path(Path::new(path)).is_none(),
        "{path} should not be classified after backend deletion"
    );
}

/// Verifies the gate does not classify itself as backend code.
///
/// Inputs:
/// - The classification gate source path.
///
/// Output:
/// - Test passes when the source path is not treated as an Erlang/BEAM backend
///   migration candidate.
///
/// Transformation:
/// - Prevents the quality gate's own filename from forcing a meaningless
///   self-classification.
#[test]
fn scanner_ignores_classification_gate_itself() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/erlang_backend_classification.rs"
    )));
}

/// Verifies the scanner ignores the OTP reference inventory gate.
///
/// Inputs:
/// - The OTP reference inventory quality gate source path.
///
/// Output:
/// - Test passes when the path is not treated as Erlang/BEAM backend code.
///
/// Transformation:
/// - Keeps reference-inventory policy files separate from backend migration
///   implementation paths even though their names contain `otp`.
#[test]
fn scanner_ignores_otp_reference_inventory_gate() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_reference_inventory.rs"
    )));
}

/// Verifies the scanner ignores the OTP runtime-exit quality gate.
///
/// Inputs:
/// - The OTP runtime-exit quality gate source path.
///
/// Output:
/// - Test passes when the path is not treated as Erlang/BEAM backend code.
///
/// Transformation:
/// - Keeps runtime-exit policy files separate from backend migration
///   implementation paths even though their names contain `otp`.
#[test]
fn scanner_ignores_otp_runtime_exit_gate() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_runtime_exit.rs"
    )));
}

/// Verifies the scanner ignores the OTP test/pipeline inventory quality gate.
///
/// Inputs:
/// - The OTP test/pipeline inventory quality gate source path.
///
/// Output:
/// - Test passes when the path is not treated as Erlang/BEAM backend code.
///
/// Transformation:
/// - Keeps test and CI policy inventory separate from backend migration
///   implementation paths even though its name contains `otp`.
#[test]
fn scanner_ignores_otp_test_pipeline_inventory_gate() {
    assert!(!is_erlang_backend_candidate(Path::new(
        "crates/terlan/src/quality/otp_test_pipeline_inventory.rs"
    )));
}

/// Verifies summary category counts stay coherent.
///
/// Inputs:
/// - Static classification table.
///
/// Output:
/// - Test passes when category counts add up to the classified path count.
///
/// Transformation:
/// - Locks the success summary to the table contents so command output remains
///   internally consistent as categories are edited.
#[test]
fn summary_counts_cover_all_classifications() {
    let summary = summary();
    assert_eq!(
        summary.classified_count,
        summary.remove_count
            + summary.reference_only_count
            + summary.temporary_bridge_count
            + summary.historical_artifact_count
    );
}
