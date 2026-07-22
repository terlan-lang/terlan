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
    let text = format!(
        "{}\n{}",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_SOURCE_FIELDS.join("\n")
    );
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
    let text = format!(
        "{}\n{}\nBranch is authoritative.",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_SOURCE_FIELDS.join("\n")
    );
    let diagnostics = validate_package_git_source_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("branch is authoritative")),
        "diagnostics should reject branch authority claim: {diagnostics:?}"
    );
}

/// Verifies required Git source fields are enforced.
///
/// Inputs:
/// - Required terms and all source fields except resolver version.
///
/// Output:
/// - Test passes when the missing field is reported.
///
/// Transformation:
/// - Prevents the Git source contract from becoming a vague prose statement
///   without the metadata needed to reproduce resolution.
#[test]
fn package_git_source_text_rejects_missing_required_fields() {
    let fields = REQUIRED_SOURCE_FIELDS
        .iter()
        .copied()
        .filter(|field| *field != "resolver version")
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!("{}\n{fields}", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_package_git_source_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("resolver version")),
        "diagnostics should reject missing resolver version: {diagnostics:?}"
    );
}

/// Verifies placeholders are rejected from the Git source contract.
///
/// Inputs:
/// - Required terms and fields plus placeholder planning text.
///
/// Output:
/// - Test passes when placeholder text is reported.
///
/// Transformation:
/// - Keeps Git source release behavior specified by executable metadata rather
///   than future planning language.
#[test]
fn package_git_source_text_rejects_placeholder_text() {
    let text = format!(
        "{}\n{}\nTODO: add Git source checksums later.",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_SOURCE_FIELDS.join("\n")
    );
    let diagnostics = validate_package_git_source_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package Git source text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}
