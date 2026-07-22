use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::*;

#[test]
fn lean_proof_closeout_accepts_current_reproducible_family_idempotently() {
    let root = TempRepo::new("closeout_current");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");

    let first = run_lean_proof_closeout(root.path()).expect("first closeout");
    let second = run_lean_proof_closeout(root.path()).expect("second closeout");

    assert_eq!(first.family_count, 1);
    assert_eq!(first.baseline_count, 8);
    assert_eq!(first.baseline_hash, second.baseline_hash);
}

#[test]
fn lean_proof_closeout_schema_accepts_all_lifecycle_statuses() {
    let families = VALID_STATUSES
        .iter()
        .map(|status| FamilyStatus {
            family: format!("family-{status}"),
            feature_class: "coreir".to_string(),
            theorem_identity: vec!["Terlan.Core.theorem".to_string()],
            proof_status: (*status).to_string(),
            last_executed_digest: valid_digest(),
            reproducibility_verdict: if *status == "current" {
                "pass".to_string()
            } else {
                "not-run".to_string()
            },
            blockers: Vec::new(),
            remediation_gates: if *status == "current" {
                Vec::new()
            } else {
                vec!["proof_repro_check".to_string()]
            },
        })
        .collect::<Vec<_>>();

    let diagnostics = validate_family_schema(&families);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn lean_proof_closeout_rejects_current_blocker_with_stable_id() {
    let root = TempRepo::new("closeout_blocker");
    write_complete_fixture(
        root.path(),
        vec!["proof_gap[artifact-drift]".to_string()],
        "pass",
        "current",
    );

    let error = run_lean_proof_closeout(root.path()).expect_err("blocker must fail");

    assert!(error.contains("error[lean_proof_closeout_blocker]"));
}

#[test]
fn lean_proof_closeout_rejects_nondeterministic_family_with_stable_id() {
    let root = TempRepo::new("closeout_nondeterministic");
    write_complete_fixture(
        root.path(),
        vec!["proof_gap[nondeterministic]".to_string()],
        "not-run",
        "nondeterministic",
    );

    let error = run_lean_proof_closeout(root.path()).expect_err("status must fail");

    assert!(error.contains("error[lean_proof_closeout_status]"));
    assert!(error.contains("error[lean_proof_closeout_reproducibility]"));
}

fn write_complete_fixture(root: &Path, blockers: Vec<String>, reproducibility: &str, status: &str) {
    fs::create_dir_all(root.join("proofs/lean/Terlan")).expect("proof tree");
    fs::create_dir_all(root.join("build/artifacts")).expect("artifact directory");
    fs::write(root.join(TOOLCHAIN), "leanprover/lean4:v4.31.0\n").expect("toolchain");
    fs::write(
        root.join(LAKE_MANIFEST),
        "{\"version\":\"1.1.0\",\"packages\":[]}",
    )
    .expect("lockfile");
    let remediation = if status == "current" {
        Vec::new()
    } else {
        vec!["proof_repro_check"]
    };
    let gate = json!({
        "families": [{
            "family": "coreir-arithmetic",
            "feature_class": "coreir",
            "theorem_identity": ["Terlan.Core.theorem"],
            "proof_status": status,
            "last_executed_digest": valid_digest(),
            "reproducibility_verdict": reproducibility,
            "blockers": blockers,
            "remediation_gates": remediation,
        }]
    });
    fs::write(
        root.join(GATE_REPORT),
        serde_json::to_string_pretty(&gate).expect("gate JSON"),
    )
    .expect("gate report");

    let mut baseline = String::from("feature_class\texpected_status\tlast_confirmed_hash\n");
    for class in EXPECTED_CLASSES {
        if *class == "coreir" {
            baseline.push_str(&format!("coreir\t{status}\t{}\n", valid_digest()));
        } else {
            baseline.push_str(&format!("{class}\tincomplete\tnone\n"));
        }
    }
    fs::write(root.join(BASELINE), baseline).expect("baseline");
}

fn valid_digest() -> String {
    format!("sha256:{}", "a".repeat(64))
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{unique}"));
        fs::create_dir_all(&path).expect("temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
