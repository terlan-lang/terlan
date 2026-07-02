use std::path::{Path, PathBuf};

use super::*;

/// Verifies Terlan fenced blocks are extracted with source locations.
///
/// Inputs:
/// - Markdown containing one complete module example and one shell block.
///
/// Output:
/// - One Terlan documentation block with its opening fence line.
///
/// Transformation:
/// - Ignores non-Terlan fences while preserving the Terlan body verbatim.
#[test]
fn extracts_terlan_fenced_blocks_with_locations() {
    let markdown = "# Example\n\n```terlan\nmodule docs.Example.\n\npub value(): Int ->\n    1.\n```\n\n```sh\nterlc run .\n```\n";

    let blocks = extract_terlan_doc_blocks(Path::new("README.md"), markdown);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].path, PathBuf::from("README.md"));
    assert_eq!(blocks[0].line, 3);
    assert_eq!(blocks[0].language, "terlan");
    assert!(blocks[0].is_complete_module());
}

/// Verifies grammar fragments are inventoried without being promoted to full
/// module examples.
///
/// Inputs:
/// - Markdown containing a Terlan expression fragment.
///
/// Output:
/// - One block classified as a fragment.
///
/// Transformation:
/// - Keeps language-design snippets visible to the gate while avoiding false
///   compiler checks for intentionally incomplete examples.
#[test]
fn classifies_non_module_blocks_as_fragments() {
    let markdown = "```terlan\n{ name: \"Ada\" }\n```\n";

    let blocks = extract_terlan_doc_blocks(Path::new("docs/grammar/README.md"), markdown);

    assert_eq!(blocks.len(), 1);
    assert!(!blocks[0].is_complete_module());
}

/// Verifies stale VM-pivot terms are rejected inside Terlan examples.
///
/// Inputs:
/// - A Terlan block containing removed runtime syntax.
///
/// Output:
/// - Diagnostics naming the stale term.
///
/// Transformation:
/// - Prevents examples from reintroducing BEAM/OTP-era source contracts.
#[test]
fn rejects_stale_runtime_terms_in_terlan_examples() {
    let blocks = vec![TerlanDocBlock {
        path: PathBuf::from("README.md"),
        line: 10,
        language: "terlan".to_string(),
        body: "import std.beam.Agent.\n\npub value(): Unit ->\n    Unit().\n".to_string(),
    }];

    let diagnostics = validate_terlan_doc_blocks(&blocks);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("std.beam")),
        "expected std.beam diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Unit()")),
        "expected Unit() diagnostic: {diagnostics:?}"
    );
}

/// Verifies the retired `.tl` fence spelling is rejected.
///
/// Inputs:
/// - A Terlan block extracted from an old `tl` fence.
///
/// Output:
/// - One diagnostic requiring the current fence spelling.
///
/// Transformation:
/// - Keeps documentation aligned with the `.terl` source extension pivot.
#[test]
fn rejects_stale_tl_fence_language() {
    let blocks = vec![TerlanDocBlock {
        path: PathBuf::from("docs/guide.md"),
        line: 4,
        language: "tl".to_string(),
        body: "module docs.Guide.\n".to_string(),
    }];

    let diagnostics = validate_terlan_doc_blocks(&blocks);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("fence language `tl`")),
        "expected tl fence diagnostic: {diagnostics:?}"
    );
}
