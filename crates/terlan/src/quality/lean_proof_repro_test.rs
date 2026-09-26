use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const TRACK_REPORT_PATH: &str = "target/quality/proof-artifacts/lean-proof-track.json";
const BASELINE_PATH: &str = "target/quality/proof-artifacts/lean-proof-baseline.tsv";

#[test]
fn native_boundary_scope_owns_native_boundary_baseline_class() {
    assert_eq!(feature_class("NativeBoundary"), "native-boundary");
}

#[test]
fn lean_proof_repro_probes_each_successful_toolchain_once_per_cycle() {
    let toolchain = ToolchainContract {
        lean_version: "4.19.0".into(),
        elan_channel: "leanprover/lean4:v4.19.0".into(),
        lake_flags: vec!["env".into(), "lean".into()],
    };
    let mut validated = BTreeSet::new();
    let mut launches = 0;
    for _ in 0..13 {
        validate_toolchain_once(&mut validated, &toolchain, || {
            launches += 1;
            Ok(())
        })
        .expect("shared toolchain validation");
    }
    assert_eq!(launches, 1);

    let mut changed = toolchain.clone();
    changed.lean_version = "4.20.0".into();
    validate_toolchain_once(&mut validated, &changed, || {
        launches += 1;
        Ok(())
    })
    .expect("distinct toolchain validation");
    assert_eq!(launches, 2);

    validated.clear();
    validate_toolchain_once(&mut validated, &toolchain, || {
        launches += 1;
        Ok(())
    })
    .expect("new cycle probes again");
    assert_eq!(launches, 3);
}

#[test]
fn lean_proof_repro_does_not_reuse_failed_toolchain_probes() {
    let toolchain = ToolchainContract {
        lean_version: "4.19.0".into(),
        elan_channel: "leanprover/lean4:v4.19.0".into(),
        lake_flags: vec!["env".into(), "lean".into()],
    };
    let mut validated = BTreeSet::new();
    assert_eq!(
        validate_toolchain_once(&mut validated, &toolchain, || {
            Err("probe failed".into())
        }),
        Err("probe failed".into())
    );
    assert!(validated.is_empty());
    validate_toolchain_once(&mut validated, &toolchain, || Ok(())).expect("probe retry");
    assert!(validated.contains(&toolchain));
}

#[test]
fn lean_proof_repro_reports_preserve_checked_in_evidence() {
    let root = TempRepo::new("immutable_proof_source");
    fs::create_dir_all(root.path().join("build/artifacts")).expect("retained evidence directory");
    root.write(
        "build/artifacts/lean-proof-baseline.tsv",
        "retained baseline\n",
    );
    root.write(
        "build/artifacts/lean-proof-gate.json",
        "{\"historical\":true}\n",
    );

    write_reports(
        &ReportPaths::repository(root.path()),
        &[],
        &[proof_status("coreir", "sha256:aaaa")],
        &json!({"gap_count": 0}),
    )
    .expect("generate current proof evidence");

    assert_eq!(
        fs::read_to_string(root.path().join("build/artifacts/lean-proof-baseline.tsv"))
            .expect("retained baseline"),
        "retained baseline\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("build/artifacts/lean-proof-gate.json"))
            .expect("retained report"),
        "{\"historical\":true}\n"
    );
    let current: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.path().join(TRACK_REPORT_PATH)).expect("current report"),
    )
    .expect("current report JSON");
    assert_eq!(current["families"][0]["family"], "coreir");
    assert_eq!(current["schema"], "terlan.lean-proof-track.v1");
    assert_eq!(current["proof_gap_metrics"]["gap_count"], 0);
    assert!(fs::read_to_string(root.path().join(BASELINE_PATH))
        .expect("current baseline")
        .contains("sha256:aaaa"));
}

#[test]
fn lean_proof_repro_replaces_track_fields_without_mutating_lane_evidence() {
    let root = TempRepo::new("proof_report_ownership");
    fs::create_dir_all(root.path().join("target/quality/proof-artifacts"))
        .expect("report directory");
    let gate = "target/quality/proof-artifacts/lean-proof-gate.json";
    let sealed =
        "{\"schema\":\"terlan.lean-proof-gate.v1\",\"lane_checksums\":{\"parser\":\"old\"}}\n";
    root.write(gate, sealed);
    // Old complete or corrupt track bytes are outputs, never composition inputs.
    for stale in [sealed, "{incomplete"] {
        root.write(TRACK_REPORT_PATH, stale);
        write_reports(
            &ReportPaths::repository(root.path()),
            &[],
            &[proof_status("coreir", "sha256:new")],
            &json!({"gap_count": 0}),
        )
        .expect("replace the track from current results");
        let current: Value =
            serde_json::from_str(&fs::read_to_string(root.path().join(TRACK_REPORT_PATH)).unwrap())
                .unwrap();
        assert_eq!(current["schema"], "terlan.lean-proof-track.v1");
        assert_eq!(current["families"][0]["last_executed_digest"], "sha256:new");
        assert_eq!(current["proof_gap_metrics"], json!({"gap_count": 0}));
        assert!(current.get("lane_checksums").is_none());
        assert_eq!(fs::read_to_string(root.path().join(gate)).unwrap(), sealed);
    }
}

#[test]
fn lean_proof_repro_stages_every_report_without_replacing_verified_outputs() {
    let root = TempRepo::new("proof_staged_output_set");
    let published = ReportPaths::repository(root.path());
    fs::create_dir_all(published.replay.parent().unwrap()).unwrap();
    for path in [&published.replay, &published.track, &published.baseline] {
        fs::write(path, "verified earlier output").unwrap();
    }
    let staging = root
        .path()
        .join("target/quality/preparation/candidate/proof.work");
    let staged = ReportPaths {
        replay: staging.join("output.json"),
        track: staging.join("output-1.json"),
        baseline: staging.join("output-2.json"),
    };
    write_reports(
        &staged,
        &[],
        &[proof_status("coreir", "sha256:current")],
        &json!({"gap_count": 0}),
    )
    .expect("write the complete private set");
    let track: Value = serde_json::from_slice(&fs::read(&staged.track).unwrap()).unwrap();
    assert_eq!(
        track["families"][0]["last_executed_digest"],
        "sha256:current"
    );
    assert!(fs::read_to_string(&staged.baseline)
        .unwrap()
        .contains("sha256:current"));
    for path in [&published.replay, &published.track, &published.baseline] {
        assert_eq!(fs::read_to_string(path).unwrap(), "verified earlier output");
    }
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
fn lean_proof_repro_attributes_missing_command_to_bounded_process_owner() {
    let error = run_proof_command(
        &mut Command::new("terlan-missing-lean-fixture-934762"),
        Duration::from_secs(1),
    )
    .expect_err("missing executable");
    assert!(error.contains("Lean proof process"), "{error}");
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
fn lean_proof_repro_preserves_shared_lake_config_and_family_build_state() {
    let root = TempRepo::new("clean_build_state");
    let lake_config = root.path().join("proofs/lean/.lake/config/0");
    let lake_build = root.path().join("proofs/lean/.lake/build/lib");
    let family_build = root.path().join("build/tmp/lean-proof/coreir-arithmetic");
    for directory in [&lake_config, &lake_build, &family_build] {
        fs::create_dir_all(directory).expect("build-state directory");
        fs::write(directory.join("generated"), "cache").expect("build-state file");
    }

    root.write("proofs/lean/lakefile.lean", "import Lake\n");
    root.write("proofs/lean/lake-manifest.json", "{\"packages\":[]}\n");
    root.write("proofs/lean/lean-toolchain", "leanprover/lean4:v4.31.0\n");
    let first = ProofWorkspace::create(root.path(), &[]).expect("first private replay");
    let second = ProofWorkspace::create(root.path(), &[]).expect("second private replay");
    assert_ne!(first.root(), second.root());
    assert!(!first.root().join("proofs/lean/.lake").exists());
    first.close().expect("clean first private workspace");
    assert!(second.root().exists());
    second.close().expect("clean second private workspace");
    for directory in [&lake_config, &lake_build, &family_build] {
        assert_eq!(
            fs::read_to_string(directory.join("generated")).unwrap(),
            "cache"
        );
    }
}

#[test]
fn lean_proof_repro_baseline_aggregates_families_in_one_class() {
    let root = TempRepo::new("shared_baseline_class");
    fs::create_dir_all(root.path().join("target/quality/proof-artifacts"))
        .expect("artifact directory");
    let statuses = vec![
        proof_status("coreir-arithmetic", "sha256:bbbb"),
        proof_status("shape-implication", "sha256:aaaa"),
    ];

    write_baseline(&root.path().join(BASELINE_PATH), &statuses).expect("write baseline");

    let baseline = fs::read_to_string(root.path().join(BASELINE_PATH)).expect("read baseline");
    assert!(baseline.contains("coreir\tcurrent\tsha256:aaaa;sha256:bbbb\n"));
}

#[test]
fn lean_proof_repro_baseline_rejects_mixed_class_statuses() {
    let root = TempRepo::new("mixed_baseline_class");
    fs::create_dir_all(root.path().join("target/quality/proof-artifacts"))
        .expect("artifact directory");
    let mut stale = proof_status("shape-implication", "sha256:aaaa");
    stale.proof_status = "stale".to_string();

    let error = write_baseline(
        &root.path().join(BASELINE_PATH),
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

#[test]
fn lean_proof_repro_snapshot_rejects_source_dependency_and_manifest_drift() {
    for relative in [
        "proofs/lean/Proof.lean",
        "proofs/lean/lean-toolchain",
        "contract.txt",
    ] {
        let root = TempRepo::new("snapshot_drift");
        fs::create_dir_all(root.path().join("proofs/lean")).unwrap();
        root.write("proofs/lean/Proof.lean", "example : True := True.intro\n");
        root.write("proofs/lean/lakefile.lean", "import Lake\n");
        root.write("proofs/lean/lake-manifest.json", "{\"packages\":[]}\n");
        root.write("proofs/lean/lean-toolchain", "leanprover/lean4:v4.31.0\n");
        root.write("contract.txt", "contract\n");
        let dependencies = vec!["proofs/lean/lean-toolchain".to_string()];
        let metadata: ReplayMetadata = serde_json::from_value(json!({
            "schema": "terlan.lean-proof-replay.v1", "family": "snapshot",
            "theorem_names": ["proof"],
            "manifest_fingerprints": {"contract.txt": sha256_file(&root.path().join("contract.txt")).unwrap()},
            "dependency_files": dependencies,
            "proof_dependency_set_hash": dependency_set_hash(root.path(), &dependencies).unwrap(),
            "source_digest": sha256_file(&root.path().join("proofs/lean/Proof.lean")).unwrap(),
            "execution_command": ["lake", "env", "lean", "Proof.lean"],
            "working_directory": "proofs/lean", "deterministic_timestamp_strategy": "none-content-addressed",
            "output_signature": {"stdout_class": "empty", "stderr_class": "empty", "exit_class": "success"},
            "toolchain": {"lean_version": "4.31.0", "elan_channel": "leanprover/lean4:v4.31.0", "lake_flags": ["env", "lean"]}
        })).unwrap();
        let workspace = ProofWorkspace::create(root.path(), &["contract.txt".into()]).unwrap();
        validate_snapshot(workspace.root(), &metadata).unwrap();
        fs::write(workspace.root().join(relative), "changed\n").unwrap();
        assert!(
            validate_snapshot(workspace.root(), &metadata).is_err(),
            "{relative}"
        );
        workspace.close().unwrap();
    }
}

#[cfg(unix)]
#[test]
fn lean_proof_repro_private_process_has_closed_stdin_and_cleans_failed_output() {
    let root = TempRepo::new("failed_process");
    fs::create_dir_all(root.path().join("proofs/lean")).unwrap();
    root.write("proofs/lean/lakefile.lean", "import Lake\n");
    root.write("proofs/lean/lake-manifest.json", "{\"packages\":[]}\n");
    root.write("proofs/lean/lean-toolchain", "leanprover/lean4:v4.31.0\n");
    let workspace = ProofWorkspace::create(root.path(), &[]).unwrap();
    let path = workspace.root().to_path_buf();
    let output = run_proof_command(Command::new("sh")
        .args(["-c", "if read value; then exit 9; fi; mkdir -p .lake/build; printf generated > .lake/build/output; exit 7"])
        .current_dir(workspace.root().join("proofs/lean")), Duration::from_secs(2)).unwrap();
    assert_eq!(output.status.code(), Some(7));
    workspace.close().unwrap();
    assert!(!path.exists());
    assert!(!root.path().join("proofs/lean/.lake").exists());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn lean_proof_repro_bounds_term_ignoring_tool_and_pipe_lifetime() {
    let start = std::time::Instant::now();
    let result = run_proof_command(
        Command::new("sh").args(["-c", "trap '' TERM; sleep 20"]),
        Duration::from_millis(100),
    );
    assert!(result.unwrap_err().contains("timed out"));
    assert!(start.elapsed() < Duration::from_secs(3));
}

fn rejection_metadata() -> ReplayMetadata {
    ReplayMetadata {
        schema: "terlan.lean-proof-replay.v1".into(),
        family: "rejection-fixture".into(),
        theorem_names: vec!["reject".into()],
        manifest_fingerprints: BTreeMap::new(),
        dependency_files: Vec::new(),
        proof_dependency_set_hash: String::new(),
        source_digest: String::new(),
        execution_command: vec![
            "lake".into(),
            "env".into(),
            "lean".into(),
            "Reject.lean".into(),
        ],
        working_directory: "proofs/lean".into(),
        deterministic_timestamp_strategy: "none-content-addressed".into(),
        output_signature: OutputSignature {
            stdout_class: "text".into(),
            stderr_class: "empty".into(),
            exit_class: "failure".into(),
        },
        toolchain: ToolchainContract {
            lean_version: "4.31.0".into(),
            elan_channel: "leanprover/lean4:v4.31.0".into(),
            lake_flags: vec!["env".into(), "lean".into()],
        },
    }
}

#[test]
fn lean_proof_repro_preflights_later_metadata_before_any_lean_process() {
    let root = TempRepo::new("proof_preflight");
    let mut metadata = rejection_metadata();
    metadata.proof_dependency_set_hash = dependency_set_hash(root.path(), &[]).unwrap();
    metadata.source_digest = metadata.proof_dependency_set_hash.clone();
    root.write("first.json", &serde_json::to_string(&metadata).unwrap());
    let artifact = |replay_metadata: &str| ArtifactRow {
        path: "proofs/lean/Reject.lean".into(),
        status: "current".into(),
        theorem_scope: "rejection".into(),
        targeted_manifests: Vec::new(),
        expected_exit: 1,
        stderr_class: "none".into(),
        proof_digest: metadata.source_digest.clone(),
        replay_metadata: replay_metadata.into(),
        remediation_plan: "none".into(),
    };
    let error = run_proof_reproducibility(
        root.path(),
        &[artifact("first.json"), artifact("missing-second.json")],
        &json!({}),
        &ReportPaths::repository(root.path()),
    )
    .unwrap_err();
    assert!(error.contains("missing-second.json"), "{error}");
    assert!(error.contains("failed to read replay metadata"), "{error}");
    // A premature probe would attempt to snapshot the missing Lean project.
    assert!(!root.path().join("target").exists());
    let mut invalid = metadata.clone();
    invalid.execution_command[3] = "Unrelated.lean".into();
    root.write("second.json", &serde_json::to_string(&invalid).unwrap());
    let error = run_proof_reproducibility(
        root.path(),
        &[artifact("first.json"), artifact("second.json")],
        &json!({}),
        &ReportPaths::repository(root.path()),
    )
    .unwrap_err();
    assert!(
        error.contains("execution command proof argument"),
        "{error}"
    );
    assert!(!root.path().join("target").exists());
}

#[test]
fn lean_proof_repro_rejects_wrong_exit_and_signal_without_second_launch() {
    for exit in [-1, 0, 2, 127] {
        let mut launches = 0;
        let error = validate_replay_pair(&rejection_metadata(), 1, || {
            launches += 1;
            Ok(NormalizedExecution {
                exit,
                stdout: "expected rejection".into(),
                stderr: String::new(),
            })
        })
        .unwrap_err();
        assert!(
            error.contains(&format!("expected 1, found {exit}")),
            "{error}"
        );
        assert_eq!(launches, 1);
    }
}

#[test]
fn lean_proof_repro_rejects_wrong_output_without_second_launch() {
    let mut launches = 0;
    let error = validate_replay_pair(&rejection_metadata(), 1, || {
        launches += 1;
        Ok(NormalizedExecution {
            exit: 1,
            stdout: String::new(),
            stderr: String::new(),
        })
    })
    .unwrap_err();
    assert!(error.contains("output signature mismatch"), "{error}");
    assert_eq!(launches, 1);
}

#[test]
fn lean_proof_repro_validates_both_independent_replicas() {
    for second_exit in [1, 2, -1] {
        let mut launches = 0;
        let result = validate_replay_pair(&rejection_metadata(), 1, || {
            launches += 1;
            Ok(NormalizedExecution {
                exit: if launches == 1 { 1 } else { second_exit },
                stdout: "expected rejection".into(),
                stderr: String::new(),
            })
        });
        assert_eq!(launches, 2);
        if second_exit == 1 {
            let (first, second) = result.unwrap();
            assert_eq!(first, second);
        } else {
            assert!(result.unwrap_err().contains("exit mismatch"));
        }
    }
}

#[test]
fn lean_proof_repro_rejects_distinct_output_with_matching_classes() {
    let mut launches = 0;
    let error = validate_replay_pair(&rejection_metadata(), 1, || {
        launches += 1;
        Ok(NormalizedExecution {
            exit: 1,
            stdout: format!("rejection {launches}"),
            stderr: String::new(),
        })
    })
    .unwrap_err();
    assert!(error.contains("proof_gap[nondeterministic]"), "{error}");
    assert_eq!(launches, 2);
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
