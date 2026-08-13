use super::*;

/// Verifies boundary documentation accepts required terms.
///
/// Inputs:
/// - A compact text containing all required terms.
///
/// Output:
/// - Test passes when no missing-term diagnostics are produced.
///
/// Transformation:
/// - Exercises the documentation term contract without filesystem access.
#[test]
fn boundary_doc_text_accepts_required_terms() {
    let text = REQUIRED_DOC_TERMS.join("\n");
    assert!(validate_boundary_doc_text(&text).is_empty());
}

/// Verifies boundary documentation reports missing required terms.
///
/// Inputs:
/// - Text without the external repository boundary wording.
///
/// Output:
/// - Test passes when the missing active-dependency term is reported.
///
/// Transformation:
/// - Prevents the old VM repository from drifting back into active dependency
///   language.
#[test]
fn boundary_doc_text_rejects_missing_terms() {
    let diagnostics = validate_boundary_doc_text("old vm checkout");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("not an active compiler dependency")),
        "diagnostics should report missing boundary wording: {diagnostics:?}"
    );
}

/// Verifies boundary documentation rejects placeholder wording.
///
/// Inputs:
/// - Complete required terms plus placeholder planning text.
///
/// Output:
/// - Test passes when placeholder text is reported.
///
/// Transformation:
/// - Keeps the external VM repository boundary as an executable release
///   contract rather than unfinished roadmap prose.
#[test]
fn boundary_doc_text_rejects_placeholder_wording() {
    let text = format!(
        "{}\nTODO: decide repository dependency policy later.",
        REQUIRED_DOC_TERMS.join("\n")
    );
    let diagnostics = validate_boundary_doc_text(&text);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder external VM boundary text")),
        "diagnostics should reject placeholder text: {diagnostics:?}"
    );
}

/// Verifies reference path validation rejects unexpected files.
///
/// Inputs:
/// - One allowed path and one arbitrary source path.
///
/// Output:
/// - Test passes when only the arbitrary path is diagnosed.
///
/// Transformation:
/// - Keeps the scanner allow-list strict while allowing documented migration
///   evidence.
#[test]
fn reference_path_validation_rejects_unexpected_files() {
    let diagnostics = validate_reference_paths(&[
        "Makefile".to_string(),
        "crates/terlan/src/runtime/vm/main.rs".to_string(),
    ]);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("runtime/vm/main.rs"));
}

/// Verifies Ambitious can be mentioned as reference text but not declared.
///
/// Inputs:
/// - Cargo-like dependency lines and free-form documentation text.
///
/// Output:
/// - Test passes when dependency syntax is rejected and prose is ignored.
///
/// Transformation:
/// - Keeps Ambitious as a behavioral checklist instead of a VM dependency.
#[test]
fn ambitious_dependency_detection_accepts_reference_text_only() {
    assert!(cargo_metadata_declares_ambitious("ambitious = \"0.1\""));
    assert!(cargo_metadata_declares_ambitious("name = \"ambitious\""));
    assert!(!cargo_metadata_declares_ambitious(
        "Ambitious is a reference checklist only."
    ));
}

/// Verifies Make target body extraction stops at the next target.
///
/// Inputs:
/// - A compact Makefile with two targets.
///
/// Output:
/// - Test passes when only the requested target body is returned.
///
/// Transformation:
/// - Protects release-train wiring checks from accidentally reading following
///   targets.
#[test]
fn make_target_body_extracts_one_target() {
    let body = make_target_body("first:\n\tone\nsecond:\n\ttwo\n", "first");
    assert!(body.contains("one"));
    assert!(!body.contains("two"));
}
