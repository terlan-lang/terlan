use std::path::PathBuf;

use super::internal_doc_findings;

/// Verifies internal planning names are detected in published docs paths.
///
/// Inputs:
/// - Fixture docs paths containing roadmap and scratch terms.
///
/// Output:
/// - Internal-doc findings for forbidden planning terms.
///
/// Transformation:
/// - Scans every path component case-insensitively.
#[test]
fn internal_doc_findings_rejects_planning_terms_in_paths() {
    let paths = vec![
        PathBuf::from("docs/guide.md"),
        PathBuf::from("docs/ROADMAP_0_0_5.md"),
        PathBuf::from("docs/compiler/scratch_notes.md"),
    ];

    let findings = internal_doc_findings(&paths);

    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].path, PathBuf::from("docs/ROADMAP_0_0_5.md"));
    assert_eq!(findings[0].term, "roadmap");
    assert_eq!(
        findings[1].path,
        PathBuf::from("docs/compiler/scratch_notes.md")
    );
    assert_eq!(findings[1].term, "scratch");
}

/// Verifies release-facing docs paths are accepted.
///
/// Inputs:
/// - Fixture docs paths without internal planning terms.
///
/// Output:
/// - Empty findings list.
///
/// Transformation:
/// - Confirms normal published documentation names remain valid.
#[test]
fn internal_doc_findings_accepts_release_facing_docs() {
    let paths = vec![
        PathBuf::from("docs/grammar/TERLAN_SYNTAX_SPEC.ebnf"),
        PathBuf::from("docs/guides/install.md"),
        PathBuf::from("docs/api/http.md"),
    ];

    let findings = internal_doc_findings(&paths);

    assert!(findings.is_empty());
}
