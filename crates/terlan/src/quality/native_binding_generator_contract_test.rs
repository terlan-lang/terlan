use super::*;

/// Verifies complete contract text is accepted.
///
/// Inputs:
/// - Synthetic text containing all required supported-input and rejection terms.
///
/// Output:
/// - Empty diagnostics.
///
/// Transformation:
/// - Locks the text-level contract validator independently from the filesystem.
#[test]
fn native_binding_generator_contract_accepts_required_terms() {
    let text = format!(
        "{}\n{}\n",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_REJECTION_TERMS.join("\n")
    );

    let diagnostics = validate_native_binding_generator_contract_text(&text);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies supported input terms are required.
///
/// Inputs:
/// - Contract text that mentions only rejection terms.
///
/// Output:
/// - Diagnostic requiring the curated wrapper surface.
///
/// Transformation:
/// - Prevents the contract from becoming only a list of rejected native shapes.
#[test]
fn native_binding_generator_contract_rejects_missing_supported_inputs() {
    let diagnostics =
        validate_native_binding_generator_contract_text(&REQUIRED_REJECTION_TERMS.join("\n"));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("curated wrapper surface")),
        "expected supported-input diagnostic: {diagnostics:?}"
    );
}

/// Verifies rejected native shapes are required.
///
/// Inputs:
/// - Contract text that mentions only supported input terms.
///
/// Output:
/// - Diagnostic requiring arbitrary C++ template rejection.
///
/// Transformation:
/// - Keeps the binding generator from silently accepting unsafe native shapes.
#[test]
fn native_binding_generator_contract_rejects_missing_rejection_terms() {
    let diagnostics = validate_native_binding_generator_contract_text(&REQUIRED_TERMS.join("\n"));

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("arbitrary C++ templates")),
        "expected rejection-term diagnostic: {diagnostics:?}"
    );
}

/// Verifies unsafe binding claims are rejected.
///
/// Inputs:
/// - Complete contract text plus a forbidden mock-sufficiency claim.
///
/// Output:
/// - Diagnostic naming the forbidden claim.
///
/// Transformation:
/// - Prevents future docs from weakening the NativeBoundary safety model.
#[test]
fn native_binding_generator_contract_rejects_forbidden_claims() {
    let text = format!(
        "{}\n{}\nmocking the native library is sufficient\n",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_REJECTION_TERMS.join("\n")
    );

    let diagnostics = validate_native_binding_generator_contract_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("mocking the native library is sufficient")),
        "expected forbidden-claim diagnostic: {diagnostics:?}"
    );
}
