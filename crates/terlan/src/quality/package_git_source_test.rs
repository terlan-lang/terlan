use super::*;

/// Verifies the Git source validator accepts required terms.
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
fn package_git_source_text_accepts_required_terms() {
    let text = REQUIRED_TERMS.join("\n");
    assert!(validate_package_git_source_text(&text).is_empty());
}

/// Verifies floating Git authority claims are rejected.
///
/// Inputs:
/// - Required terms plus a forbidden floating-source claim.
///
/// Output:
/// - Test passes when the forbidden claim is reported.
///
/// Transformation:
/// - Prevents branches, tags, latest commits, or target package manager Git
///   resolution from replacing immutable Terlan revisions.
#[test]
fn package_git_source_text_rejects_forbidden_claims() {
    let text = format!("{}\nBranch is authoritative.", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_package_git_source_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("branch is authoritative")),
        "diagnostics should reject branch authority claim: {diagnostics:?}"
    );
}
