use crate::terlan_quality::support::make_quality_temp_dir;

use super::*;

fn root(name: &str) -> PathBuf {
    make_quality_temp_dir(name)
}

fn write(root: &Path, relative: &str, value: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
    fs::write(path, value).expect("write fixture");
}

fn evidence(gate: Abi1ReleaseGate, runs: Value) -> String {
    serde_json::to_string(&json!({
        "schema": EVIDENCE_SCHEMA,
        "gate": gate.id(),
        "abi_version": 1,
        "managed_layout_profile": 1,
        "status": "passed",
        "revision": "0123456789abcdef",
        "runs": runs,
    }))
    .expect("serialize fixture")
}

#[test]
fn fuzz_gate_requires_seeded_failure_free_corpus_evidence() {
    let root = root("abi1_fuzz_evidence");
    let runs = json!([
        {"seed": 1, "cases": 4000, "failures": 0, "corpus_digest": "a".repeat(64)},
        {"seed": 2, "cases": 4000, "failures": 0, "corpus_digest": "b".repeat(64)},
        {"seed": 3, "cases": 4000, "failures": 0, "corpus_digest": "c".repeat(64)}
    ]);
    write(
        &root,
        "target/abi1-evidence/continuous-fuzz.json",
        &evidence(Abi1ReleaseGate::ContinuousFuzz, runs),
    );
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::ContinuousFuzz)
            .expect("valid fuzz evidence")
            .case_count,
        3
    );
    let bad = json!([{"seed": 1, "cases": 10000, "failures": 1, "corpus_digest": "a".repeat(64)}]);
    write(
        &root,
        "target/abi1-evidence/continuous-fuzz.json",
        &evidence(Abi1ReleaseGate::ContinuousFuzz, bad),
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::ContinuousFuzz).is_err());
}

#[test]
fn cross_target_gate_requires_two_supported_architectures() {
    let root = root("abi1_cross_target_evidence");
    let runs = json!([
        {"target": "x86_64-unknown-linux-gnu", "architecture": "x86_64", "pointer_width": 64, "endian": "little", "failures": 0, "status": "passed"},
        {"target": "aarch64-unknown-linux-gnu", "architecture": "aarch64", "pointer_width": 64, "endian": "little", "failures": 0, "status": "passed"}
    ]);
    write(
        &root,
        "target/abi1-evidence/cross-target-conformance.json",
        &evidence(Abi1ReleaseGate::CrossTargetConformance, runs),
    );
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::CrossTargetConformance)
            .expect("valid target evidence")
            .case_count,
        2
    );
}

#[test]
fn tail_latency_gate_enforces_ordered_bounded_percentiles() {
    let root = root("abi1_tail_latency_evidence");
    let runs = json!([{"workload": "managed-message-roundtrip", "samples": 10000, "p95_ns": 800, "p99_ns": 1200, "p95_limit_ns": 1000, "p99_limit_ns": 1500}]);
    write(
        &root,
        "target/abi1-evidence/tail-latency.json",
        &evidence(Abi1ReleaseGate::TailLatency, runs),
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::TailLatency).is_ok());
    let bad = json!([{"workload": "managed-message-roundtrip", "samples": 10000, "p95_ns": 1600, "p99_ns": 1200, "p95_limit_ns": 1000, "p99_limit_ns": 1500}]);
    write(
        &root,
        "target/abi1-evidence/tail-latency.json",
        &evidence(Abi1ReleaseGate::TailLatency, bad),
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::TailLatency).is_err());
}

#[test]
fn specialization_gate_rejects_semantic_drift() {
    let root = root("abi1_specialization_evidence");
    let runs = json!([{"semantic_case": "aggregate-roundtrip", "generic_digest": "d".repeat(64), "specialized_digest": "d".repeat(64), "generic_status": "passed", "specialized_status": "passed"}]);
    write(
        &root,
        "target/abi1-evidence/specialization-equivalence.json",
        &evidence(Abi1ReleaseGate::SpecializationEquivalence, runs),
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::SpecializationEquivalence).is_ok());
    let bad = json!([{"semantic_case": "aggregate-roundtrip", "generic_digest": "d".repeat(64), "specialized_digest": "e".repeat(64), "generic_status": "passed", "specialized_status": "passed"}]);
    write(
        &root,
        "target/abi1-evidence/specialization-equivalence.json",
        &evidence(Abi1ReleaseGate::SpecializationEquivalence, bad),
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::SpecializationEquivalence).is_err());
}

#[test]
fn zero_copy_gate_requires_implementation_and_behavioral_owners() {
    let root = root("abi1_zero_copy");
    write(&root, "crates/terlan/src/runtime/native_image/managed/sequences.rs", "Borrowed semantic view of a managed bitstring slice\nReturns a zero-copy byte slice when both boundaries are byte aligned\nself.storage.get(start..end)");
    write(&root, "crates/terlan/src/runtime/native_image/managed/managed_sequence_test.rs", "binary_slices_enforce_bounds_and_bit_order\nsequence_graph_survives_precise_relocation\ntyped_sequence_access_rejects_wrong_and_foreign_references");
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::ZeroCopyConformance)
            .expect("complete zero-copy owners")
            .case_count,
        6
    );
}

#[test]
fn trusted_adapter_gate_rejects_in_process_unsafe_code() {
    let root = root("abi1_adapter_audit");
    for relative in [
        "crates/terlan/src/runtime/native_boundary/mod.rs",
        "crates/terlan/src/runtime/vm/native_boundary/mod.rs",
        "crates/terlan/src/runtime/vm/capability_worker/mod.rs",
        "crates/terlan/src/native_worker/mod.rs",
    ] {
        write(&root, relative, "pub fn checked() {}\n");
    }
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::TrustedAdapterAudit)
            .expect("safe adapters")
            .case_count,
        4
    );
    write(
        &root,
        "crates/terlan/src/runtime/native_boundary/mod.rs",
        "pub unsafe fn bypass() {}\n",
    );
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::TrustedAdapterAudit).is_err());
}

#[test]
fn release_candidate_requires_every_prerequisite_report() {
    let root = root("abi1_release_candidate");
    for gate in PREREQUISITE_GATES {
        write(
            &root,
            gate.report_path().to_str().expect("report path"),
            &serde_json::to_string(&json!({"schema": REPORT_SCHEMA, "gate": gate.id(), "abi_version": 1, "managed_layout_profile": 1, "status": "validated", "case_count": 1, "revision": "0123456789abcdef"})).expect("serialize report"),
        );
    }
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::ReleaseCandidate)
            .expect("complete candidate")
            .case_count,
        PREREQUISITE_GATES.len()
    );
    fs::remove_file(root.join(PREREQUISITE_GATES[0].report_path())).expect("remove report");
    assert!(run_abi1_release_gate(&root, Abi1ReleaseGate::ReleaseCandidate).is_err());
}

#[test]
fn compatibility_freeze_binds_candidate_to_explicit_contract_terms() {
    let root = root("abi1_compatibility_freeze");
    write(
        &root,
        Abi1ReleaseGate::ReleaseCandidate.report_path().to_str().expect("candidate path"),
        &serde_json::to_string(&json!({"schema": REPORT_SCHEMA, "gate": "release-candidate", "abi_version": 1, "managed_layout_profile": 1, "status": "validated", "case_count": 6, "revision": "0123456789abcdef"})).expect("serialize candidate"),
    );
    write(
        &root,
        "docs/runtime/ABI1_COMPATIBILITY_BASELINE.json",
        &serde_json::to_string(&json!({"schema": "terlan.abi1.compatibility-baseline.v1", "status": "frozen", "abi_version": 1, "managed_layout_profile": 1, "release_revision": "0123456789abcdef", "contract_terms": ["ABI 1", "managed-layout profile 1"]})).expect("serialize baseline"),
    );
    write(
        &root,
        "docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md",
        "ABI 1\nmanaged-layout profile 1\n",
    );
    assert_eq!(
        run_abi1_release_gate(&root, Abi1ReleaseGate::CompatibilityFreeze)
            .expect("valid freeze")
            .case_count,
        2
    );
}
