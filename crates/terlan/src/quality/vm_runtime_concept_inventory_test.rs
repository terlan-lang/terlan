use super::{run_vm_runtime_concept_inventory, validate_document_text};

/// Verifies the checked-in VM concept inventory satisfies the gate.
///
/// Inputs:
/// - Repository root.
///
/// Output:
/// - Test passes when required concept counts are present.
///
/// Transformation:
/// - Exercises the same document-backed path used by `terlan-quality`.
#[test]
fn vm_runtime_concept_inventory_accepts_checked_in_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let summary =
        run_vm_runtime_concept_inventory(&root).expect("checked-in VM runtime concept inventory");

    assert_eq!(summary.concept_count, 28);
    assert_eq!(summary.required_vm_semantics_count, 16);
    assert_eq!(summary.library_abstraction_count, 4);
    assert_eq!(summary.distribution_machinery_count, 4);
    assert_eq!(summary.unsupported_otp_compatibility_count, 4);
}

/// Verifies missing concept terms are diagnosed.
///
/// Inputs:
/// - Minimal malformed concept text.
///
/// Output:
/// - Test passes when required concept diagnostics are emitted.
///
/// Transformation:
/// - Calls text validation directly so diagnostics stay stable without
///   creating temporary files.
#[test]
fn vm_runtime_concept_inventory_rejects_missing_concepts() {
    let diagnostics = validate_document_text("required-vm-semantics");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("scheduler reductions")),
        "expected scheduler reductions diagnostic, got {diagnostics:?}"
    );
}

/// Verifies forbidden compatibility claims are rejected.
///
/// Inputs:
/// - Text containing a forbidden BEAM-parity claim.
///
/// Output:
/// - Test passes when the forbidden claim is reported.
///
/// Transformation:
/// - Keeps OTP compatibility drift blocked at the concept-inventory layer.
#[test]
fn vm_runtime_concept_inventory_rejects_forbidden_claims() {
    let diagnostics =
        validate_document_text("required-vm-semantics beam opcode parity is required");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("beam opcode parity is required")),
        "expected forbidden claim diagnostic, got {diagnostics:?}"
    );
}
