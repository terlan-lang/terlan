use super::*;

fn artifact(name: &str) -> ArtifactRow {
    ArtifactRow {
        path: format!("proofs/lean/{name}.lean"),
        status: "current".into(),
        theorem_scope: "CoreIR".into(),
        targeted_manifests: vec!["manifest".into()],
        expected_exit: 0,
        stderr_class: "none".into(),
        proof_digest: "digest".into(),
        replay_metadata: "metadata".into(),
        remediation_plan: "none".into(),
    }
}

#[test]
fn selected_proofs_preserve_inventory_order_without_unrequested_work() {
    let selected = select(
        vec![artifact("A"), artifact("B"), artifact("C")],
        &[artifact("C").path, artifact("A").path],
    )
    .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        ["proofs/lean/A.lean", "proofs/lean/C.lean"]
    );
}

#[test]
fn selected_proofs_reject_empty_unknown_and_duplicate_requests() {
    for paths in [
        vec![],
        vec![artifact("B").path],
        vec![artifact("A").path, artifact("A").path],
    ] {
        assert!(select(vec![artifact("A")], &paths).is_err());
    }
    assert!(select(vec![artifact("A"), artifact("A")], &[artifact("A").path]).is_err());
}

#[test]
fn selected_proofs_reject_aliases_traversal_and_nonproof_paths() {
    for path in [
        "/proofs/lean/A.lean",
        "proofs/lean/../A.lean",
        "proofs/lean/./A.lean",
        "proofs/lean//A.lean",
        "proofs/lean/A\\B.lean",
        "proofs/lean/A.txt",
        "proofs/leanish/A.lean",
    ] {
        let mut row = artifact("A");
        row.path = path.into();
        assert!(select(vec![row], &[path.into()]).is_err(), "{path}");
    }
}

#[test]
fn selected_proofs_cannot_turn_negative_or_unready_evidence_into_success() {
    let mut nondeterministic = artifact("A");
    nondeterministic.status = "nondeterministic".into();
    let mut negative = artifact("A");
    negative.expected_exit = 1;
    let mut remediation = artifact("A");
    remediation.remediation_plan = "repair".into();
    for row in [nondeterministic, negative, remediation] {
        assert!(select(vec![row], &[artifact("A").path]).is_err());
    }
}
