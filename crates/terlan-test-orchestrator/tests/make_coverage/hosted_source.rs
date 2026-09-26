//! Consume real completed records through a mock previously authenticated cache.
//! Signature verification belongs to the downloader tests, not this fixture.

use super::*;
use serde_json::json;

fn digest(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn checkpoint(root: &Path, revision: &str, summary: &Value) -> String {
    let context = serde_json::to_vec(&json!({
        "schema":"terlan.hosted-download-cache.v1",
        "repository":"fixture/repository", "revision":revision,
        "implementation":digest(&fs::read(root.join("scripts/download_validated_release_artifacts.sh")).unwrap()),
        "release":{"id":21,"attempt":1,"path":".github/workflows/release.yml"},
        "compiler":{"id":123456,"attempt":2,"job_id":1,"path":".github/workflows/ci.yml"}
    })).unwrap();
    let key = digest(&context);
    let cache = format!("target/publication-downloads/{key}");
    let candidate = serde_json::to_vec(&json!({
        "schema":"terlan.hosted-candidate-validation.v1", "decision":"pass",
        "source_revision":revision,"workflow":".github/workflows/ci.yml","run_id":123456,
        "coverage":{"cache_key":key,"producer_job_id":1,"producer_attempt":2,
            "authentication":"github-attestation","records":summary}
    }))
    .unwrap();
    write(
        root,
        "target/quality/hosted-candidate-validation.json",
        &candidate,
    );
    let mut entries = BTreeMap::from([
        ("context.json".to_owned(), context),
        (
            "evidence/hosted-candidate-validation.json".to_owned(),
            candidate,
        ),
    ]);
    for name in summary["files"].as_object().unwrap().keys() {
        entries.insert(
            format!("coverage/{name}"),
            fs::read(root.join("target/quality").join(name)).unwrap(),
        );
    }
    let mut manifest = String::new();
    for (path, bytes) in entries {
        manifest.push_str(&format!("{}  ./{path}\n", digest(&bytes)));
        write(root, &format!("{cache}/{path}"), bytes);
    }
    write(root, &format!("{cache}/verified-files.sha256"), manifest);
    write(root, "target/publication-inputs.lock", b"");
    cache
}

fn invoke(root: &Path, goal: &str) -> bool {
    run(command(root, root.join("target/driver")).args([
        "--with-hosted-cargo-coverage",
        "--",
        "make",
        "--no-print-directory",
        goal,
    ]))
}

fn report(root: &Path) -> Value {
    serde_json::from_slice(
        &fs::read(root.join("target/quality/hosted-source-coverage.json")).unwrap(),
    )
    .unwrap()
}

/// Reuses the parent fixture's completed test bodies; no second suite is built or run.
pub(super) fn exercise(root: &Path, revision: &str, summary: &Value) {
    let cache = checkpoint(root, revision, summary);
    super::aot::exercise(root);
    let bodies = fs::read(root.join("target/bodies.txt")).unwrap();
    assert!(invoke(root, "gates"), "valid hosted source gates");
    let evidence = report(root);
    assert_eq!(evidence["decision"], "hosted-source-gates-covered");
    assert_eq!(evidence["direct_cargo_launch_count"], 0);
    let phases = evidence["phases"].as_array().unwrap();
    let owners = phases
        .iter()
        .filter(|phase| phase["executor"] == "hosted-source-covered-gates")
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    let coverage = &owners[0]["test_execution"];
    assert_eq!(
        coverage["scope"],
        "hosted-canonical-source-test-coverage-v1"
    );
    assert_eq!(coverage["local_artifact_test_equivalence"], false);
    assert_eq!(coverage["requester_process_count"], 4);
    assert_eq!(coverage["checkpoint"]["records"], *summary);
    assert_eq!(
        coverage["checkpoint"]["distribution_bytes_revalidated"],
        false
    );
    let before_entries = fs::read_to_string(root.join("target/gate-entries")).unwrap();
    assert!(invoke(root, "publish-evidence-source-prerequisites"));
    assert_eq!(report(root)["decision"], "hosted-source-gates-covered");
    assert_eq!(
        fs::read_to_string(root.join("target/gate-entries")).unwrap(),
        format!("{before_entries}normal\n"),
        "overlapping production multicore/AOT prerequisites must share one gate execution"
    );
    assert!(
        !run(command(root, "make").args([
            "--no-print-directory",
            "publish-evidence-source-prerequisites"
        ])),
        "unowned publication source prerequisites must fail"
    );
    super::publication_graph::exercise(root);
    assert!(invoke(root, "semantic"));
    assert!(invoke(root, "proof"));
    for goal in [
        "missing",
        "changed",
        "no-requests",
        "swallowed",
        "semantic-wrong-root",
        "proof-wrong-root",
    ] {
        assert!(!invoke(root, goal), "accepted {goal}");
        assert_ne!(report(root)["decision"], "hosted-source-gates-covered");
    }
    let source = fs::read(root.join("terlan/src/lib.rs")).unwrap();
    assert!(
        !invoke(root, "hosted-change-source"),
        "accepted mutation during Make"
    );
    write(root, "terlan/src/lib.rs", &source);

    // Admission faults must not enter Make even when the requested gate is valid.
    let entries = fs::read(root.join("target/gate-entries")).unwrap();
    write(root, "terlan/src/lib.rs", b"// changed source\n");
    assert!(!invoke(root, "normal"));
    write(root, "terlan/src/lib.rs", &source);
    let candidate_path = "target/quality/hosted-candidate-validation.json";
    let candidate = fs::read(root.join(candidate_path)).unwrap();
    write(root, candidate_path, b"{}");
    assert!(!invoke(root, "normal"));
    write(root, candidate_path, &candidate);
    let manifest_path = format!("{cache}/verified-files.sha256");
    let manifest = fs::read(root.join(&manifest_path)).unwrap();
    for corrupt in [b"not a manifest\n".as_slice(), b"", b"incomplete"] {
        write(root, &manifest_path, corrupt);
        assert!(!invoke(root, "normal"));
    }
    write(root, &manifest_path, &manifest);
    let selection_path = format!("{cache}/coverage/rust-test-suite-report.json.selections.json");
    let selection = fs::read(root.join(&selection_path)).unwrap();
    write(root, &selection_path, b"{}");
    assert!(!invoke(root, "normal"));
    write(root, &selection_path, selection);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("target/publication-inputs.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(!invoke(root, "normal"), "accepted active checkpoint writer");
    lock.unlock().unwrap();
    assert_eq!(fs::read(root.join("target/gate-entries")).unwrap(), entries);
    assert_eq!(fs::read(root.join("target/bodies.txt")).unwrap(), bodies);
}
