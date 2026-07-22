use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

#[test]
fn native_boundary_scope_owns_native_boundary_baseline_class() {
    assert_eq!(feature_class("NativeBoundary"), "native-boundary");
}

#[test]
fn lean_proof_repro_dependency_hash_is_ordered_and_content_addressed() {
    let root = TempRepo::new("dependency_hash");
    root.write("a.lock", "alpha\n");
    root.write("b.lock", "beta\n");
    let paths = vec!["a.lock".to_string(), "b.lock".to_string()];

    let first = dependency_set_hash(root.path(), &paths).expect("first hash");
    let second = dependency_set_hash(root.path(), &paths).expect("second hash");
    assert_eq!(first, second);

    root.write("b.lock", "changed\n");
    let changed = dependency_set_hash(root.path(), &paths).expect("changed hash");
    assert_ne!(first, changed);

    let reversed = vec!["b.lock".to_string(), "a.lock".to_string()];
    let error = dependency_set_hash(root.path(), &reversed).expect_err("order must fail");
    assert!(error.contains("byte-lexically sorted"));
}

#[test]
fn lean_proof_repro_normalizes_paths_and_line_endings() {
    let root = Path::new("/tmp/terlan-proof-root");
    let normalized = normalized_execution_from_parts(
        root,
        0,
        b"/tmp/terlan-proof-root/proof\r\n".to_vec(),
        Vec::new(),
    );

    assert_eq!(normalized.stdout, "<repo>/proof");
    assert!(normalized.stderr.is_empty());
    assert_eq!(normalized.exit, 0);
}

#[test]
fn lean_proof_repro_signature_detects_output_drift() {
    let first = NormalizedExecution {
        exit: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    let changed = NormalizedExecution {
        exit: 0,
        stdout: "warning order changed".to_string(),
        stderr: String::new(),
    };

    assert_eq!(execution_signature(&first), execution_signature(&first));
    assert_ne!(execution_signature(&first), execution_signature(&changed));
}

#[test]
fn lean_proof_repro_reports_missing_command_as_lean_unavailable() {
    let error = unavailable_error(
        "missing-lake",
        std::io::Error::new(std::io::ErrorKind::NotFound, "missing"),
    );

    assert!(error.starts_with("lean_unavailable:"), "{error}");
}

#[test]
fn lean_proof_repro_manifest_drift_reports_recorded_then_actual_digest() {
    assert_eq!(
        manifest_drift_diagnostic(
            "docs/grammar/TERLAN_SYNTAX_SPEC.ebnf",
            Some("sha256:recorded"),
            "sha256:actual",
        ),
        "proof_gap[manifest-drift]: manifest fingerprint drift for `docs/grammar/TERLAN_SYNTAX_SPEC.ebnf`: expected `sha256:recorded`, found `sha256:actual`"
    );
}

#[test]
fn lean_proof_repro_dependency_drift_reports_recorded_then_actual_digest() {
    assert_eq!(
        dependency_drift_diagnostic("sha256:recorded", "sha256:actual"),
        "proof_gap[dependency-drift]: proof dependency set drift: expected `sha256:recorded`, found `sha256:actual`"
    );
}

#[test]
fn lean_proof_repro_cleans_lake_config_and_family_build_state() {
    let root = TempRepo::new("clean_build_state");
    let lake_config = root.path().join("proofs/lean/.lake/config/0");
    let lake_build = root.path().join("proofs/lean/.lake/build/lib");
    let family_build = root.path().join("build/tmp/lean-proof/coreir-arithmetic");
    for directory in [&lake_config, &lake_build, &family_build] {
        fs::create_dir_all(directory).expect("build-state directory");
        fs::write(directory.join("generated"), "cache").expect("build-state file");
    }

    clean_proof_build_state(root.path(), "coreir-arithmetic").expect("clean proof state");

    assert!(!lake_config.exists());
    assert!(!lake_build.exists());
    assert!(!family_build.exists());
}

#[test]
fn lean_proof_repro_baseline_aggregates_families_in_one_class() {
    let root = TempRepo::new("shared_baseline_class");
    fs::create_dir_all(root.path().join("build/artifacts")).expect("artifact directory");
    let statuses = vec![
        proof_status("coreir-arithmetic", "sha256:bbbb"),
        proof_status("shape-implication", "sha256:aaaa"),
    ];

    write_baseline(root.path(), &statuses).expect("write baseline");

    let baseline = fs::read_to_string(root.path().join(BASELINE_PATH)).expect("read baseline");
    assert!(baseline.contains("coreir\tcurrent\tsha256:aaaa;sha256:bbbb\n"));
}

#[test]
fn lean_proof_repro_baseline_rejects_mixed_class_statuses() {
    let root = TempRepo::new("mixed_baseline_class");
    fs::create_dir_all(root.path().join("build/artifacts")).expect("artifact directory");
    let mut stale = proof_status("shape-implication", "sha256:aaaa");
    stale.proof_status = "stale".to_string();

    let error = write_baseline(
        root.path(),
        &[proof_status("coreir-arithmetic", "sha256:bbbb"), stale],
    )
    .expect_err("mixed statuses must fail");

    assert!(error.contains("mixed statuses"));
}

fn proof_status(family: &str, digest: &str) -> ProofFamilyStatus {
    ProofFamilyStatus {
        family: family.to_string(),
        feature_class: "coreir".to_string(),
        theorem_identity: vec!["Terlan.Core.theorem".to_string()],
        proof_status: "current".to_string(),
        last_executed_digest: digest.to_string(),
        reproducibility_verdict: "pass".to_string(),
        blockers: Vec::new(),
        remediation_gates: Vec::new(),
    }
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

    fn write(&self, relative: &str, text: &str) {
        fs::write(self.path.join(relative), text).expect("fixture file");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
