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
    assert_eq!(first.lane_count, 8);
    assert_eq!(first.baseline_count, 9);
    assert_eq!(first.baseline_hash, second.baseline_hash);
}

#[test]
fn lean_proof_closeout_accepts_multiple_current_families_in_one_class() {
    let root = TempRepo::new("closeout_shared_class");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");
    let second_digest = format!("sha256:{}", "b".repeat(64));
    let gate_path = root.path().join(GATE_REPORT);
    let mut gate: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&gate_path).expect("read gate report"))
            .expect("parse gate report");
    gate["families"]
        .as_array_mut()
        .expect("family array")
        .push(json!({
            "family": "shape-implication",
            "feature_class": "coreir",
            "theorem_identity": ["Terlan.Type.ShapeImplication.theorem"],
            "proof_status": "current",
            "last_executed_digest": second_digest,
            "reproducibility_verdict": "pass",
            "blockers": [],
            "remediation_gates": [],
        }));
    fs::write(
        gate_path,
        serde_json::to_string_pretty(&gate).expect("serialize gate report"),
    )
    .expect("write gate report");
    let baseline_path = root.path().join(BASELINE);
    let baseline = fs::read_to_string(&baseline_path).expect("read baseline");
    fs::write(
        baseline_path,
        baseline.replace(
            &format!("coreir\tcurrent\t{}", valid_digest()),
            &format!("coreir\tcurrent\t{};{second_digest}", valid_digest()),
        ),
    )
    .expect("write baseline");

    let summary = run_lean_proof_closeout(root.path()).expect("shared class closeout");

    assert_eq!(summary.family_count, 2);
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

#[test]
fn lean_proof_closeout_rejects_lane_checksum_missing_from_gate_report() {
    let root = TempRepo::new("closeout_lane_checksum");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");
    let gate_path = root.path().join(GATE_REPORT);
    let mut gate: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&gate_path).expect("read gate"))
            .expect("parse gate");
    gate["lane_checksums"]
        .as_object_mut()
        .expect("lane checksums")
        .remove("wasm");
    fs::write(
        gate_path,
        serde_json::to_string_pretty(&gate).expect("serialize gate"),
    )
    .expect("write gate");

    let error = run_lean_proof_closeout(root.path()).expect_err("missing checksum must fail");

    assert!(error.contains("error[lean_proof_closeout_lane_checksum]"));
}

#[test]
fn lean_proof_closeout_rejects_lane_level_blocker() {
    let root = TempRepo::new("closeout_lane_blocker");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");
    let report_path = root.path().join(LANE_REPORT);
    let mut report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read lanes"))
            .expect("parse lanes");
    report["lanes"][0]["blockers"] = json!(["synthetic-proof"]);
    fs::write(
        report_path,
        serde_json::to_string_pretty(&report).expect("serialize lanes"),
    )
    .expect("write lanes");

    let error = run_lean_proof_closeout(root.path()).expect_err("lane blocker must fail");

    assert!(error.contains("error[lean_proof_closeout_lane_blocker]"));
}

#[test]
fn lean_proof_closeout_rejects_unresolved_open_gap_metric() {
    let root = TempRepo::new("closeout_open_gap");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");
    let gate_path = root.path().join(GATE_REPORT);
    let mut gate: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&gate_path).expect("read gate"))
            .expect("parse gate");
    gate["proof_gap_metrics"]["unresolved_open_count"] = json!(1);
    fs::write(
        gate_path,
        serde_json::to_string_pretty(&gate).expect("serialize gate"),
    )
    .expect("write gate");

    let error = run_lean_proof_closeout(root.path()).expect_err("open gap must fail");

    assert!(error.contains("error[lean_proof_closeout_open_gap]"));
}

#[test]
fn lean_proof_closeout_rejects_proof_runtime_smoke_mismatch() {
    let root = TempRepo::new("closeout_smoke_mismatch");
    write_complete_fixture(root.path(), Vec::new(), "pass", "current");
    let report_path = root.path().join(SMOKE_REPORT);
    let mut report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read smoke"))
            .expect("parse smoke");
    report["results"][0]["runtime_status"] = json!("failure");
    report["blockers"] = json!([{"smoke_id": "semantic-chain"}]);
    fs::write(
        report_path,
        serde_json::to_string_pretty(&report).expect("serialize smoke"),
    )
    .expect("write smoke");

    let error = run_lean_proof_closeout(root.path()).expect_err("mismatch must fail");

    assert!(error.contains("error[lean_proof_closeout_smoke]"));
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
    let lane_checksum = format!("sha256:{}", "c".repeat(64));
    let matrix_checksum = format!("sha256:{}", "d".repeat(64));
    let lane_checksums = EXPECTED_LANES
        .iter()
        .map(|lane| ((*lane).to_string(), json!(lane_checksum)))
        .collect::<serde_json::Map<_, _>>();
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
        }],
        "lane_matrix_checksum": matrix_checksum,
        "lane_checksums": lane_checksums,
        "proof_gap_metrics": {
            "unresolved_open_count": 0
        },
    });
    fs::write(
        root.join(GATE_REPORT),
        serde_json::to_string_pretty(&gate).expect("gate JSON"),
    )
    .expect("gate report");
    let lanes = EXPECTED_LANES
        .iter()
        .enumerate()
        .map(|(index, lane)| {
            let executable = index < 6;
            json!({
                "lane": lane,
                "severity": "hard",
                "status": "pass",
                "coverage_status": if executable { "executable_current" } else { "accepted_gap" },
                "duration_ms": 1,
                "duration_tolerance_ms": 100,
                "number_of_families": if executable { 1 } else { 0 },
                "failed_families": [],
                "gap_count": 1,
                "nondeterministic_count": 0,
                "reproducibility_failures": 0,
                "smoke_health_score": 100,
                "smoke_policy_minimum": 100,
                "blockers": [],
                "checksum": lane_checksum,
            })
        })
        .collect::<Vec<_>>();
    let lane_report = json!({
        "schema": "terlan.lean-proof-lanes.v1",
        "lane_matrix_checksum": matrix_checksum,
        "lanes": lanes,
    });
    fs::write(
        root.join(LANE_REPORT),
        serde_json::to_string_pretty(&lane_report).expect("lane JSON"),
    )
    .expect("lane report");
    let lane_health = EXPECTED_LANES
        .iter()
        .map(|lane| ((*lane).to_string(), json!(100)))
        .collect::<serde_json::Map<_, _>>();
    let smoke_report = json!({
        "schema": "terlan.lean-proof-smoke.v1",
        "policy_minimum": 100,
        "compatibility_status": "pass",
        "lane_health": lane_health,
        "blockers": [],
        "results": [{
            "smoke_id": "semantic-chain",
            "proof_status": "pass",
            "runtime_status": "pass",
            "compatibility_status": "stable",
            "health_score": 100
        }]
    });
    fs::write(
        root.join(SMOKE_REPORT),
        serde_json::to_string_pretty(&smoke_report).expect("smoke JSON"),
    )
    .expect("smoke report");

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
