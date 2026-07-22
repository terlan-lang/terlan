use super::*;

/// Verifies the contract text validator accepts required Hex metadata terms.
///
/// Inputs:
/// - A compact text containing every required term.
///
/// Output:
/// - Test passes when no diagnostics are produced.
///
/// Transformation:
/// - Exercises the term gate without reading repository files.
#[test]
fn hex_target_metadata_text_accepts_required_terms() {
    let text = REQUIRED_TERMS.join("\n");
    assert!(validate_hex_target_metadata_text(&text).is_empty());
}

/// Verifies Hex compatibility claims are rejected.
///
/// Inputs:
/// - Required terms plus a forbidden Hex/OTP claim.
///
/// Output:
/// - Test passes when the forbidden claim is reported.
///
/// Transformation:
/// - Prevents package docs from redefining Hex as the Terlan runtime contract.
#[test]
fn hex_target_metadata_text_rejects_forbidden_claims() {
    let text = format!("{}\nHex implies OTP.", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_hex_target_metadata_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("hex implies otp")),
        "diagnostics should reject Hex/OTP compatibility claim: {diagnostics:?}"
    );
}

/// Verifies required target metadata terms cannot be dropped silently.
///
/// Inputs:
/// - Required terms except `compiler target selection`.
///
/// Output:
/// - Diagnostic naming the missing target-selection contract term.
///
/// Transformation:
/// - Keeps package metadata tied to compiler-owned target selection instead of
///   target package managers.
#[test]
fn hex_target_metadata_text_rejects_missing_target_selection_term() {
    let text = REQUIRED_TERMS
        .iter()
        .copied()
        .filter(|term| *term != "compiler target selection")
        .collect::<Vec<_>>()
        .join("\n");

    let diagnostics = validate_hex_target_metadata_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("compiler target selection")),
        "diagnostics should reject missing compiler target selection term: {diagnostics:?}"
    );
}

/// Verifies package metadata cannot contain placeholder roadmap language.
///
/// Inputs:
/// - Required terms plus a placeholder TODO marker.
///
/// Output:
/// - Diagnostic naming placeholder metadata text.
///
/// Transformation:
/// - Prevents release package contracts from passing with unfinished prose.
#[test]
fn hex_target_metadata_text_rejects_placeholders() {
    let text = format!("{}\nTODO: document later.", REQUIRED_TERMS.join("\n"));

    let diagnostics = validate_hex_target_metadata_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package metadata term")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}
