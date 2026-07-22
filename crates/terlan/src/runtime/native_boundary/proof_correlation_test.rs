use super::*;

#[test]
fn native_boundary_proof_correlation_accepts_current_replay_metadata() {
    let proof = native_boundary_proof_correlation().expect("current proof correlation");

    assert_eq!(proof.family, "native-boundary");
    assert_eq!(proof.proof_path, PROOF_PATH);
    assert!(proof.proof_digest.starts_with("sha256:"));
    assert_eq!(proof.proof_digest.len(), 71);
}

#[test]
fn native_boundary_proof_correlation_rejects_source_digest_drift() {
    let different = format!("{}\n-- drift", PROOF_SOURCE);

    let error = correlate_native_boundary_proof(REPLAY_METADATA, &different)
        .err()
        .expect("source drift must fail");
    assert!(error.contains("proof source digest drift"));
}

#[test]
fn native_boundary_proof_correlation_rejects_unknown_schema() {
    let metadata = REPLAY_METADATA.replace(
        "terlan.lean-proof-replay.v1",
        "terlan.lean-proof-replay.future",
    );

    let error = correlate_native_boundary_proof(&metadata, PROOF_SOURCE)
        .err()
        .expect("unknown schema must fail");
    assert!(error.contains("unsupported NativeBoundary proof replay schema"));
}

#[test]
fn native_boundary_proof_correlation_rejects_wrong_family() {
    let metadata = REPLAY_METADATA.replace(
        "\"family\": \"native-boundary\"",
        "\"family\": \"unrelated\"",
    );

    let error = correlate_native_boundary_proof(&metadata, PROOF_SOURCE)
        .err()
        .expect("wrong proof family must fail");
    assert!(error.contains("proof replay family must be `native-boundary`"));
}
