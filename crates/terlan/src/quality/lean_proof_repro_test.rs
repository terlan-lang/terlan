use super::*;

fn metadata() -> ReplayMetadata {
    ReplayMetadata {
        schema: "terlan.lean-proof-replay.v2".into(),
        family: "fixture".into(),
        theorem_names: vec!["Fixture.theorem".into()],
        manifests: Vec::new(),
        dependency_files: Vec::new(),
        execution_command: vec![
            "lake".into(),
            "env".into(),
            "lean".into(),
            "Fixture.lean".into(),
        ],
        working_directory: "proofs/lean".into(),
        deterministic_timestamp_strategy: "none".into(),
        output_signature: OutputSignature {
            stdout_class: "empty".into(),
            stderr_class: "empty".into(),
            exit_class: "success".into(),
        },
        toolchain: ToolchainContract {
            lean_version: "4.31.0".into(),
            elan_channel: "leanprover/lean4:v4.31.0".into(),
            lake_flags: vec!["env".into(), "lean".into()],
        },
    }
}

fn execution(exit: i32, stdout: &str, stderr: &str) -> NormalizedExecution {
    NormalizedExecution {
        exit,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

#[test]
fn replay_pair_accepts_identical_semantic_results() {
    let mut calls = 0;
    let result = validate_replay_pair(&metadata(), 0, || {
        calls += 1;
        Ok(execution(0, "", ""))
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(result.0, result.1);
}

#[test]
fn replay_pair_rejects_nondeterministic_output() {
    let mut metadata = metadata();
    metadata.output_signature.stdout_class = "text".into();
    let mut calls = 0;
    let error = validate_replay_pair(&metadata, 0, || {
        calls += 1;
        Ok(execution(0, &calls.to_string(), ""))
    })
    .unwrap_err();
    assert!(error.contains("nondeterministic"));
}

#[test]
fn replay_pair_rejects_exit_and_output_classes() {
    assert!(
        validate_replay_pair(&metadata(), 0, || Ok(execution(1, "", "")))
            .unwrap_err()
            .contains("exit mismatch")
    );
    assert!(
        validate_replay_pair(&metadata(), 0, || Ok(execution(0, "text", "")))
            .unwrap_err()
            .contains("output class mismatch")
    );
}

#[test]
fn normalization_removes_workspace_and_platform_newlines() {
    let root = Path::new("/private/proof-workspace");
    assert_eq!(
        normalized_execution_from_parts(
            root,
            0,
            b" /private/proof-workspace/file\r\n".to_vec(),
            b"\r\n".to_vec(),
        ),
        execution(0, "<repo>/file", ""),
    );
}

#[test]
fn toolchain_probe_runs_once_per_contract_and_retries_failures() {
    let contract = metadata().toolchain;
    let mut validated = BTreeSet::new();
    let mut calls = 0;
    validate_toolchain_once(&mut validated, &contract, || {
        calls += 1;
        Ok(())
    })
    .unwrap();
    validate_toolchain_once(&mut validated, &contract, || {
        calls += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(calls, 1);

    let other = ToolchainContract {
        lean_version: "other".into(),
        ..contract
    };
    assert!(validate_toolchain_once(&mut validated, &other, || Err("failed".into())).is_err());
    assert!(!validated.contains(&other));
}

#[test]
fn status_report_contains_no_evidence_hash_column() {
    let path = std::env::temp_dir().join(format!(
        "terlan-proof-status-{}-{}.tsv",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    write_statuses(&path, &[]).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert_eq!(text.lines().next(), Some("feature_class\texpected_status"));
    assert!(!text.contains("hash"));
}
