use super::*;

/// Builds a stable file URI for import-action tests.
///
/// Inputs:
/// - No explicit input.
///
/// Output:
/// - File URI rooted outside any project-specific fixture.
///
/// Transformation:
/// - Uses a `/tmp` file path so tests exercise fallback std suggestions without
///   requiring project-local summaries.
fn test_uri() -> Url {
    Url::parse("file:///tmp/import_actions.terl").expect("test file uri")
}

/// Extracts the first text edit from an import action candidate.
///
/// Inputs:
/// - `candidate`: generated import action candidate.
///
/// Output:
/// - Text edit stored in the candidate.
///
/// Transformation:
/// - Clones the edit so assertions can inspect range and replacement text
///   without consuming the caller's candidate list.
fn candidate_edit(candidate: &ImportActionCandidate) -> TextEdit {
    candidate.edit.clone()
}

/// Verifies unresolved constructor diagnostics map to import candidates.
///
/// Inputs:
/// - A stable typechecker diagnostic message.
///
/// Output:
/// - Test assertion over generated import actions.
///
/// Transformation:
/// - Exercises the diagnostic parser used by LSP code actions.
#[test]
fn diagnostic_import_actions_recognize_unknown_constructor() {
    let actions = import_code_actions_for_diagnostic(
        &test_uri(),
        "module sample.\n\npub value(): Int ->\n  Vector(\"Alice\").\n",
        "unknown constructor Vector / 1",
    );

    assert!(actions
        .iter()
        .any(|action| action.title == "Import std.native.collections.Vector"));
}

/// Verifies missing module imports are inserted after the module header.
///
/// Inputs:
/// - A source document with no imports.
///
/// Output:
/// - Test assertion over insertion edit text and range.
///
/// Transformation:
/// - Generates the `Vector` fallback import and checks that it inserts after
///   the declaration header instead of rewriting source code.
#[test]
fn import_candidate_inserts_missing_vector_import() {
    let text = "module sample.\n\npub value(): Int ->\n  Vector(\"Alice\").\n";
    let candidates = import_candidates_for_symbol(&test_uri(), text, "Vector");
    let vector = candidates
        .iter()
        .find(|candidate| candidate.title == "Import std.native.collections.Vector")
        .expect("vector import candidate");
    let edit = candidate_edit(vector);

    assert_eq!(edit.new_text, "import std.native.collections.Vector.\n");
    assert_eq!(edit.range.start, Position::new(2, 0));
    assert_eq!(edit.range.end, Position::new(2, 0));
}

/// Verifies wrong same-leaf imports are replaced.
///
/// Inputs:
/// - A source document importing a non-existent `std.collections.Vector`.
///
/// Output:
/// - Test assertion over replacement title and text.
///
/// Transformation:
/// - Ensures the quick fix corrects stale same-leaf imports instead of adding
///   a second conflicting module alias.
#[test]
fn import_candidate_replaces_wrong_vector_import() {
    let text = "\
module sample.

import std.io.Console.{println}.
import std.collections.Vector.

pub value(): Int ->
  Vector(\"Alice\").
";
    let candidates = import_candidates_for_symbol(&test_uri(), text, "Vector");
    let vector = candidates
        .iter()
        .find(|candidate| candidate.title == "Replace import with std.native.collections.Vector")
        .expect("vector replacement candidate");
    let edit = candidate_edit(vector);

    assert_eq!(edit.new_text, "import std.native.collections.Vector.\n");
    assert_eq!(edit.range.start, Position::new(3, 0));
    assert_eq!(edit.range.end.line, 4);
}
