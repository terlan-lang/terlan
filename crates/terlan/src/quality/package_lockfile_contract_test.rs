use super::*;

/// Verifies the lockfile text validator accepts required terms.
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
fn package_lockfile_text_accepts_required_terms() {
    let text = REQUIRED_TERMS.join("\n");
    assert!(validate_package_lockfile_text(&text).is_empty());
}

/// Verifies target lockfile authority claims are rejected.
///
/// Inputs:
/// - Required terms plus a forbidden target-lockfile claim.
///
/// Output:
/// - Test passes when the forbidden claim is reported.
///
/// Transformation:
/// - Prevents target adapter lockfiles from replacing `terlan.lock`.
#[test]
fn package_lockfile_text_rejects_forbidden_claims() {
    let text = format!(
        "{}\nCargo.lock is the Terlan lockfile.",
        REQUIRED_TERMS.join("\n")
    );
    let diagnostics = validate_package_lockfile_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("cargo.lock")),
        "diagnostics should reject Cargo.lock authority claim: {diagnostics:?}"
    );
}
