use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{blocker_hash, GAP_HEADER};

/// Verifies proof PR ownership accepts a complete owner map.
///
/// Inputs:
/// - Temporary proof inventory, gap manifest, owner map, and Makefile.
///
/// Output:
/// - Summary with owner count and a generated PR report.
///
/// Transformation:
/// - Keeps unresolved proof gaps actionable by canonical owner bucket.
#[test]
fn lean_proof_pr_accepts_complete_owner_map() {
    let root = temp_repo("lean_proof_pr_accepts");
    write_fixture(&root, owners_fixture());

    let summary = run_lean_proof_pr(&root).expect("complete ownership should pass");

    assert_eq!(summary.owner_count, 2);
    assert_eq!(summary.unresolved_gap_count, 1);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("\"owner_bucket\": \"cli\""));
    assert!(report.contains("Core preservation remains unresolved"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unresolved gaps require owners.
///
/// Inputs:
/// - Owner map without a row for the gap feature.
///
/// Output:
/// - Diagnostic naming the unowned gap.
///
/// Transformation:
/// - Prevents proof debt from becoming anonymous.
#[test]
fn lean_proof_pr_rejects_missing_gap_owner() {
    let root = temp_repo("lean_proof_pr_missing_gap_owner");
    write_fixture(
        &root,
        owners_fixture().replace(
            "gap\tCore preservation\tcli\tSlice 1\tlean-proof-track-check\tRestore Core proof.\tnone\tnone\n",
            "",
        ),
    );

    let error = run_lean_proof_pr(&root).expect_err("missing owner should fail");

    assert!(error.contains("unresolved gap `Core preservation` has no owner"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies owner buckets are constrained.
///
/// Inputs:
/// - Owner map using a non-canonical bucket.
///
/// Output:
/// - Diagnostic naming the invalid owner bucket.
///
/// Transformation:
/// - Keeps proof accountability grouped into release-owned teams.
#[test]
fn lean_proof_pr_rejects_unknown_owner_bucket() {
    let root = temp_repo("lean_proof_pr_bad_owner");
    write_fixture(&root, owners_fixture().replace("\tcli\t", "\tcompiler\t"));

    let error = run_lean_proof_pr(&root).expect_err("bad owner should fail");

    assert!(error.contains("owner `compiler` is not a canonical proof owner bucket"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies required gate links must resolve to Make targets.
///
/// Inputs:
/// - Owner map referencing a missing Make target.
///
/// Output:
/// - Diagnostic naming the unknown gate.
///
/// Transformation:
/// - Keeps PR proof status linked to executable checks.
#[test]
fn lean_proof_pr_rejects_unknown_gate_link() {
    let root = temp_repo("lean_proof_pr_unknown_gate");
    write_fixture(
        &root,
        owners_fixture().replace("lean-proof-track-check", "missing-proof-check"),
    );

    let error = run_lean_proof_pr(&root).expect_err("unknown gate should fail");

    assert!(error.contains("references unknown Make gate `missing-proof-check`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies exception token and expiry are paired.
///
/// Inputs:
/// - Owner map with an exception token but no expiry.
///
/// Output:
/// - Diagnostic naming exception field consistency.
///
/// Transformation:
/// - Prevents stale PR exceptions from bypassing proof ownership gates.
#[test]
fn lean_proof_pr_rejects_unpaired_exception_fields() {
    let root = temp_repo("lean_proof_pr_unpaired_exception");
    write_fixture(
        &root,
        owners_fixture().replace("\tnone\tnone\n", "\tPR-123\tnone\n"),
    );

    let error = run_lean_proof_pr(&root).expect_err("unpaired exception should fail");

    assert!(error.contains("exception token and expiry must both be `none` or both concrete"));
    fs::remove_dir_all(root).expect("remove fixture");
}

fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(root.join("docs/compiler/proof_track")).expect("create proof track dir");
    root
}

fn write_fixture(root: &Path, owners: String) {
    fs::write(
        root.join(INVENTORY_PATH),
        format!(
            "{INVENTORY_HEADER}\nproofs/lean\tabsent\trepository proof tree\t0.0.7\tlean-proof-track-check\tNo proof tree.\n"
        ),
    )
    .expect("write inventory");
    fs::write(
        root.join(GAP_PATH),
        format!(
            "{GAP_HEADER}\nCore preservation\tblocked\tmodel_gap\tmissing proof\tcli\tlean-proof-track-check\tdeadline:0.0.7-closeout\t2026-07-16\t{}\tdocs/compiler/LEAN_PROOF_TRACK.md\n",
            blocker_hash("Core preservation", "model_gap", "missing proof", "2026-07-16")
        ),
    )
    .expect("write gaps");
    fs::write(root.join(OWNER_PATH), owners).expect("write owners");
    fs::write(
        root.join("Makefile"),
        "lean-proof-track-check:\n\tcargo test\n",
    )
    .expect("write Makefile");
}

fn owners_fixture() -> String {
    format!(
        "{OWNER_HEADER}\n\
         inventory\tproofs/lean\truntime\tSlice 1\tlean-proof-track-check\tRestore Lean proof tree.\tnone\tnone\n\
         gap\tCore preservation\tcli\tSlice 1\tlean-proof-track-check\tRestore Core proof.\tnone\tnone\n"
    )
}
