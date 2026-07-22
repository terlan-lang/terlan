use super::{
    VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
    VmNativeImageDiagnosticMetadata,
};

/// Proves native diagnostic metadata is deterministic and contains no executable body.
#[test]
fn native_image_diagnostics_are_canonical_structural_metadata() {
    let mut references = VmNativeGenerationReferenceSnapshot::new();
    references.record(VmNativeGenerationReferenceClass::Timer, 2);
    references.record(VmNativeGenerationReferenceClass::NativeFrame, 1);
    let metadata = VmNativeImageDiagnosticMetadata::new(
        "terlc:build:package:module",
        [0xab; 32],
        vec![9, 3, 9],
        7,
        &references,
    )
    .expect("valid diagnostics");

    assert_eq!(metadata.continuation_ids, [3, 9]);
    assert_eq!(metadata.generation_reference_total, 3);
    assert!(!metadata.generation_quiescent);
    assert_eq!(metadata.generation_references[0].class, "native_frames");
    assert_eq!(metadata.generation_references[1].class, "timers");
    let json = serde_json::to_string(&metadata).expect("serialize diagnostics");
    assert!(json.contains(&"ab".repeat(32)));
    for forbidden in [
        "coreIr",
        "coreIR",
        "instructions",
        "executableBytes",
        "sourcePath",
    ] {
        assert!(
            !json.contains(forbidden),
            "leaked forbidden field {forbidden}"
        );
    }
}

/// Proves malformed identities, digests, and generation epochs fail closed.
#[test]
fn native_image_diagnostics_reject_invalid_identity_inputs() {
    let references = VmNativeGenerationReferenceSnapshot::new();
    assert!(VmNativeImageDiagnosticMetadata::new("", [1; 32], vec![], 1, &references).is_err());
    assert!(
        VmNativeImageDiagnosticMetadata::new("image", [0; 32], vec![], 1, &references).is_err()
    );
    assert!(
        VmNativeImageDiagnosticMetadata::new("image", [1; 32], vec![], 0, &references).is_err()
    );
}
