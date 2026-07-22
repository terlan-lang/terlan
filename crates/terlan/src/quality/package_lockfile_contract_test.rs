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
    let text = format!(
        "{}\n{}",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_LOCKFILE_FIELDS.join("\n")
    );
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
        "{}\n{}\nCargo.lock is the Terlan lockfile.",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_LOCKFILE_FIELDS.join("\n")
    );
    let diagnostics = validate_package_lockfile_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("cargo.lock")),
        "diagnostics should reject Cargo.lock authority claim: {diagnostics:?}"
    );
}

/// Verifies required lockfile fields are enforced.
///
/// Inputs:
/// - Required terms and all fields except the resolver version field.
///
/// Output:
/// - Test passes when the missing field is reported.
///
/// Transformation:
/// - Prevents the lockfile contract from describing reproducibility without
///   naming the metadata required to reproduce resolution.
#[test]
fn package_lockfile_text_rejects_missing_required_fields() {
    let fields = REQUIRED_LOCKFILE_FIELDS
        .iter()
        .copied()
        .filter(|field| *field != "resolver version")
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!("{}\n{fields}", REQUIRED_TERMS.join("\n"));
    let diagnostics = validate_package_lockfile_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("resolver version")),
        "diagnostics should reject missing resolver version: {diagnostics:?}"
    );
}

/// Verifies placeholders are rejected from the lockfile contract.
///
/// Inputs:
/// - Required terms and fields plus placeholder planning text.
///
/// Output:
/// - Test passes when placeholder text is reported.
///
/// Transformation:
/// - Keeps the release contract executable rather than aspirational.
#[test]
fn package_lockfile_text_rejects_placeholder_text() {
    let text = format!(
        "{}\n{}\nTODO: add package source fields later.",
        REQUIRED_TERMS.join("\n"),
        REQUIRED_LOCKFILE_FIELDS.join("\n")
    );
    let diagnostics = validate_package_lockfile_text(&text);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder package lockfile text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}
