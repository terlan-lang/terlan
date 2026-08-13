use super::*;
use crate::terlan_quality::lean_proof_track::lean_proof_gap::{blocker_hash, GAP_HEADER};
use std::collections::BTreeSet;
use std::fs;

fn known_gap_gates() -> BTreeSet<String> {
    [
        "core-typing-spec-check",
        "language-feature-coverage-100-check",
        "lean-proof-track-check",
        "native-boundary-security-check",
        "target-inference-contract-check",
        "vm-runtime-semantics-check",
        "wasm-coreir-lowering-check",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn gap_rows(rows: &str) -> String {
    let rows = rows
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 5, "legacy test gap row: {line}");
            let category = "model_gap";
            let updated_at = "2026-07-16";
            format!(
                "{}\tblocked\t{category}\t{}\t{}\t{}\tdeadline:0.0.7-closeout\t{updated_at}\t{}\t{}",
                columns[0],
                columns[1],
                columns[2],
                columns[3],
                blocker_hash(columns[0], category, columns[1], updated_at),
                columns[4]
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{GAP_HEADER}\n{rows}")
}

#[test]
fn lean_proof_inventory_accepts_absent_tree_row() {
    let rows = parse_inventory(
        "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes\n\
         proofs/lean\tabsent\trepository proof tree\t0.0.7\tlean-proof-track-check\tNo proof tree.\n",
    )
    .expect("parse inventory");

    let diagnostics = validate_inventory(&rows, &[]);

    assert!(diagnostics.is_empty(), "diagnostics = {diagnostics:?}");
}

#[test]
fn lean_proof_inventory_rejects_untracked_lean_file() {
    let rows = parse_inventory(
        "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes\n\
         proofs/lean\tabsent\trepository proof tree\t0.0.7\tlean-proof-track-check\tNo proof tree.\n",
    )
    .expect("parse inventory");
    let lean_files = vec![PathBuf::from("proofs/lean/Terlan/Core.lean")];

    let diagnostics = validate_inventory(&rows, &lean_files);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing from")),
        "expected untracked file diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_inventory_rejects_current_row_when_tree_is_absent() {
    let rows = parse_inventory(
        "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes\n\
         proofs/lean\tabsent\trepository proof tree\t0.0.7\tlean-proof-track-check\tNo proof tree.\n\
         proofs/lean/Terlan/Core.lean\tcurrent\tCore preservation\t0.0.7\tlean-proof-track-check\tstale row\n",
    )
    .expect("parse inventory");

    let diagnostics = validate_inventory(&rows, &[]);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("claims status `current`")),
        "expected absent-tree stale inventory diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_inventory_rejects_missing_row_file_when_tree_exists() {
    let rows = parse_inventory(
        "path\tstatus\tsource_contract\tterlan_version\tgate\tnotes\n\
         proofs/lean/Terlan/Core.lean\tcurrent\tCore preservation\t0.0.7\tlean-proof-track-check\tcurrent row\n\
         proofs/lean/Terlan/Missing.lean\tcurrent\tMissing preservation\t0.0.7\tlean-proof-track-check\tstale row\n",
    )
    .expect("parse inventory");
    let lean_files = vec![PathBuf::from("proofs/lean/Terlan/Core.lean")];

    let diagnostics = validate_inventory(&rows, &lean_files);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("references a missing Lean proof file")),
        "expected missing proof file inventory diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_gaps_require_seed_gap_rows() {
    let rows = parse_gaps(&gap_rows("")).expect("parse gaps");

    let diagnostics = validate_gaps(&rows, &known_gap_gates());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("typed CoreIR preservation")),
        "expected required gap diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_gaps_reject_missing_make_gate() {
    let rows = parse_gaps(
        &gap_rows(
            "typed CoreIR preservation\tmissing proof\tcompiler\tmissing-proof-check\tdocs/compiler/type_spec/terlan_core_typing_spec.toml\n",
        ),
    )
    .expect("parse gaps");

    let diagnostics = validate_gaps(&rows, &known_gap_gates());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing-proof-check")),
        "expected missing gate diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_gaps_reject_unknown_owner() {
    let rows = parse_gaps(
        &gap_rows(
            "typed CoreIR preservation\tmissing proof\tsecurity\tcore-typing-spec-check\tdocs/compiler/type_spec/terlan_core_typing_spec.toml\n",
        ),
    )
    .expect("parse gaps");

    let diagnostics = validate_gaps(&rows, &known_gap_gates());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("security")),
        "expected unknown owner diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_gaps_require_coverage_manifest_links() {
    let rows = parse_gaps(&gap_rows(
        "EBNF syntax preservation\tmissing proof\tcompiler\tlean-proof-track-check\tdocs/grammar/TERLAN_SYNTAX_SPEC.ebnf\n\
         typed CoreIR preservation\tmissing proof\tcompiler\tcore-typing-spec-check\tdocs/compiler/type_spec/terlan_core_typing_spec.toml;docs/compiler/CORE_IR_LEAN_CONFORMANCE.md\n\
         target-profile inference\tmissing proof\tcompiler\ttarget-inference-contract-check\tdocs/compiler/TERLAN_TARGET_INFERENCE.md\n\
         VM execution subset\tmissing proof\tvm\tvm-runtime-semantics-check\tdocs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md\n\
         pattern and operator coverage\tmissing proof\tcompiler\tlanguage-feature-coverage-100-check\tdocs/compiler/type_spec/language_feature_coverage_matrix.json\n\
         native-boundary contracts\tmissing proof\truntime\tnative-boundary-security-check\tdocs/runtime/NATIVE_BOUNDARY_GLOSSARY.md\n\
         Wasm CoreIR lowering\tmissing proof\tcompiler\twasm-coreir-lowering-check\tcrates/terlan/src/backends/wasm/README.md\n\
         Aeneas Rust verification bridge\tmissing proof\tformal\tlean-proof-track-check\tdocs/compiler/LEAN_PROOF_TRACK.md\n",
    ))
    .expect("parse gaps");

    let diagnostics = validate_gaps(&rows, &known_gap_gates());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("operator_coverage_matrix.json")),
        "expected missing operator matrix link diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_gap_manifest_paths_reject_missing_files() {
    let rows = parse_gaps(&gap_rows(
        "EBNF syntax preservation\tmissing proof\tcompiler\tlean-proof-track-check\tdocs/compiler/missing.ebnf\n",
    ))
    .expect("parse gaps");
    let root = std::env::temp_dir().join(format!(
        "terlan_lean_gap_manifest_paths_{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove old temp root");
    }
    fs::create_dir_all(&root).expect("create temp root");

    let diagnostics = validate_gap_manifest_paths(&root, &rows);

    fs::remove_dir_all(&root).expect("remove temp root");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("docs/compiler/missing.ebnf")),
        "expected missing manifest diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_track_rejects_stale_runtime_terms() {
    let diagnostics = validate_stale_terms(
        "proofs/lean/Old.lean",
        "theorem CoreV0 : True := by trivial",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("CoreV0")),
        "expected stale-term diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_artifacts_parse_execution_contract() {
    let rows = parse_artifacts(
        "path\tstatus\ttheorem_scope\ttargeted_manifests\texpected_exit\tstderr_class\tproof_digest\treplay_metadata\tremediation_plan\n\
         proofs/lean/Terlan/Core.lean\tcurrent\tCoreIR\tdocs/core.toml;docs/core.md\t0\tnone\tsha256:abc\tproofs/lean/artifacts/core.json\tnone\n",
    )
    .expect("parse artifacts");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].theorem_scope, "CoreIR");
    assert_eq!(rows[0].targeted_manifests.len(), 2);
    assert_eq!(rows[0].expected_exit, 0);
}

#[test]
fn lean_proof_artifacts_accept_native_boundary_scope() {
    let rows = parse_artifacts(
        "path\tstatus\ttheorem_scope\ttargeted_manifests\texpected_exit\tstderr_class\tproof_digest\treplay_metadata\tremediation_plan\n\
         proofs/lean/Terlan/Runtime/NativeBoundary.lean\tcurrent\tNativeBoundary\tdocs/runtime/native.md\t0\tnone\tsha256:abc\tproofs/lean/artifacts/native.json\tnone\n",
    )
    .expect("parse NativeBoundary artifact");

    assert_eq!(rows[0].theorem_scope, "NativeBoundary");
    assert!(VALID_THEOREM_SCOPES.contains(&rows[0].theorem_scope.as_str()));
}

#[test]
fn lean_proof_artifacts_require_preservation_lowering_and_rejection_scopes() {
    let diagnostics = validate_artifacts(Path::new("."), &[], &[]);

    for required_scope in REQUIRED_CURRENT_THEOREM_SCOPES {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(required_scope)),
            "expected missing `{required_scope}` scope diagnostic, got {diagnostics:?}"
        );
    }
}

#[test]
fn lean_proof_artifacts_reject_stale_digest() {
    let root =
        std::env::temp_dir().join(format!("terlan_lean_stale_digest_{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove old temp root");
    }
    fs::create_dir_all(root.join("proofs/lean/Terlan")).expect("proof directory");
    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::write(
        root.join("proofs/lean/Terlan/Core.lean"),
        "theorem seed : True := by trivial\n",
    )
    .expect("proof");
    fs::write(root.join("docs/core.toml"), "version = 1\n").expect("manifest");
    let artifacts = vec![ArtifactRow {
        path: "proofs/lean/Terlan/Core.lean".to_string(),
        status: "current".to_string(),
        theorem_scope: "CoreIR".to_string(),
        targeted_manifests: vec!["docs/core.toml".to_string()],
        expected_exit: 0,
        stderr_class: "none".to_string(),
        proof_digest: "sha256:stale".to_string(),
        replay_metadata: "proofs/lean/artifacts/core.json".to_string(),
        remediation_plan: "none".to_string(),
    }];
    let inventory = vec![InventoryRow {
        path: artifacts[0].path.clone(),
        status: "current".to_string(),
        source_contract: "CoreIR seed".to_string(),
        terlan_version: "0.0.7".to_string(),
        gate: "lean-proof-track-check".to_string(),
        notes: "current".to_string(),
    }];

    let diagnostics = validate_artifacts(&root, &artifacts, &inventory);

    fs::remove_dir_all(&root).expect("remove temp root");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("digest is stale")),
        "expected stale digest diagnostic, got {diagnostics:?}"
    );
}

#[test]
fn lean_proof_artifacts_require_nondeterministic_remediation_plan() {
    let artifacts = vec![ArtifactRow {
        path: "proofs/lean/Terlan/Core.lean".to_string(),
        status: "nondeterministic".to_string(),
        theorem_scope: "CoreIR".to_string(),
        targeted_manifests: vec!["docs/core.toml".to_string()],
        expected_exit: 0,
        stderr_class: "none".to_string(),
        proof_digest: "sha256:unused".to_string(),
        replay_metadata: "proofs/lean/artifacts/core.json".to_string(),
        remediation_plan: "none".to_string(),
    }];
    let inventory = vec![InventoryRow {
        path: artifacts[0].path.clone(),
        status: "nondeterministic".to_string(),
        source_contract: "CoreIR seed".to_string(),
        terlan_version: "0.0.7".to_string(),
        gate: "lean-proof-track-check".to_string(),
        notes: "requires remediation".to_string(),
    }];

    let diagnostics = validate_artifacts(Path::new("."), &artifacts, &inventory);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("requires a remediation plan")),
        "expected remediation diagnostic, got {diagnostics:?}"
    );
}
