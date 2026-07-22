use super::*;
use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Verifies compact constructor diagnostics map to import candidates.
///
/// Inputs:
/// - A stable typechecker diagnostic message using compact `Name/arity`
///   spelling.
///
/// Output:
/// - Test assertion over generated import actions.
///
/// Transformation:
/// - Exercises the diagnostic parser used by LSP code actions so compact and
///   spaced arity diagnostic spellings stay equivalent for constructor fixes.
#[test]
fn diagnostic_import_actions_recognize_compact_unknown_constructor() {
    let actions = import_code_actions_for_diagnostic(
        &test_uri(),
        "module sample.\n\npub value(): Int ->\n  Vector(\"Alice\").\n",
        "unknown constructor Vector/1",
    );

    assert!(actions
        .iter()
        .any(|action| action.title == "Import std.native.collections.Vector"));
}

/// Verifies diagnostic import actions carry editor-applicable workspace edits.
///
/// Inputs:
/// - A stable unknown-constructor diagnostic for `Vector`.
/// - A source document missing the canonical Vector import.
///
/// Output:
/// - Test passes when the LSP code action is a quick fix with a single
///   workspace edit inserting `std.native.collections.Vector`.
///
/// Transformation:
/// - Exercises the actual LSP `CodeAction` surface instead of only the internal
///   import candidate helper.
#[test]
fn diagnostic_import_action_contains_workspace_edit() {
    let uri = test_uri();
    let actions = import_code_actions_for_diagnostic(
        &uri,
        "module sample.\n\npub value(): Int ->\n  Vector(\"Alice\").\n",
        "unknown constructor Vector / 1",
    );
    let action = actions
        .iter()
        .find(|action| action.title == "Import std.native.collections.Vector")
        .expect("vector code action");
    let edit = action.edit.as_ref().expect("workspace edit");
    let changes = edit.changes.as_ref().expect("workspace edit changes");
    let edits = changes.get(&uri).expect("edit for current uri");

    assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "import std.native.collections.Vector.\n");
    assert_eq!(edits[0].range.start, Position::new(2, 0));
    assert_eq!(edits[0].range.end, Position::new(2, 0));
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

/// Verifies imports are inserted after leading module documentation.
///
/// Inputs:
/// - A source document with a leading module-doc comment before the module
///   declaration.
///
/// Output:
/// - Test passes when the auto-import edit lands after the module header and
///   blank separator, not before the documentation comment.
///
/// Transformation:
/// - Protects formatter/editor parity for files that preserve leading module
///   documentation while still accepting imports in the normal header block.
#[test]
fn import_candidate_preserves_leading_module_docs() {
    let text = "\
//! Sample module docs.
module sample.

pub value(): Int ->
  Vector(\"Alice\").
";
    let candidates = import_candidates_for_symbol(&test_uri(), text, "Vector");
    let vector = candidates
        .iter()
        .find(|candidate| candidate.title == "Import std.native.collections.Vector")
        .expect("vector import candidate");
    let edit = candidate_edit(vector);

    assert_eq!(edit.new_text, "import std.native.collections.Vector.\n");
    assert_eq!(edit.range.start, Position::new(3, 0));
    assert_eq!(edit.range.end, Position::new(3, 0));
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

/// Verifies already imported modules do not produce duplicate quick fixes.
///
/// Inputs:
/// - A source document that already imports the canonical Vector module.
///
/// Output:
/// - Test passes when no Vector import candidate is produced.
///
/// Transformation:
/// - Locks auto-import to avoid no-op replacements and duplicate module imports
///   when the correct public module is already visible.
#[test]
fn import_candidate_skips_already_imported_vector() {
    let text = "\
module sample.

import std.native.collections.Vector.

pub value(): Int ->
  Vector(\"Alice\").
";
    let candidates = import_candidates_for_symbol(&test_uri(), text, "Vector");

    assert!(
        candidates.is_empty(),
        "already imported Vector should not produce candidates: {candidates:?}"
    );
}

/// Verifies provider summaries produce selected function imports.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2`.
/// - A consumer source that calls `add` without importing it.
///
/// Output:
/// - Test passes when auto-import suggests `import math.{add}.` at the header
///   insertion point.
///
/// Transformation:
/// - Exercises the same interface-summary loader used by LSP code actions so
///   package-local public functions can be repaired without fallback tables.
#[test]
fn import_candidate_inserts_selected_function_from_provider_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-function-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("math.terli"),
        "\
module math.

pub add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");
    let add = candidates
        .iter()
        .find(|candidate| candidate.title == "Import add from math")
        .expect("selected add import candidate");
    let edit = candidate_edit(add);

    assert_eq!(edit.new_text, "import math.{add}.\n");
    assert_eq!(edit.range.start, Position::new(2, 0));
    assert_eq!(edit.range.end, Position::new(2, 0));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies private provider functions do not produce auto-import fixes.
///
/// Inputs:
/// - A sibling provider source file with private function `secret/0`.
/// - A consumer source that calls `secret` without importing it.
///
/// Output:
/// - Test passes when no `secret` import candidate is produced.
///
/// Transformation:
/// - Ensures auto-import only exposes public provider interface members and
///   does not scrape private implementation details from source modules.
#[test]
fn import_candidate_rejects_private_provider_function() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-private-function-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("secrets.terl"),
        "\
module secrets.

secret(): Int ->
    1.

pub visible(): Int ->
    secret().
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(): Int ->
  secret().
";

    let candidates = import_candidates_for_symbol(&uri, text, "secret");

    assert!(
        candidates.is_empty(),
        "private provider function should not produce candidates: {candidates:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies stale re-export summaries do not produce auto-import fixes.
///
/// Inputs:
/// - A wrapper summary importing `renamed.{add}` and exporting `add/2`.
/// - No `renamed` provider artifact.
/// - A consumer source that calls `add` without importing it.
///
/// Output:
/// - Test passes when no `add` import candidate is produced.
///
/// Transformation:
/// - Prevents stale package-cache metadata from offering quick fixes whose
///   original provider source has been renamed or removed.
#[test]
fn import_candidate_rejects_stale_reexport_provider_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-stale-reexport-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("wrapper.terli"),
        "\
module wrapper.

import renamed.{add}.

export add/2.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");

    assert!(
        candidates.is_empty(),
        "stale re-export should not produce candidates: {candidates:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies generated binding summaries produce selected function imports.
///
/// Inputs:
/// - A generated-style `std/summaries/pkg.generated.Widget.typi` summary that
///   exports `render/1`.
/// - A consumer source that calls `render` without importing it.
///
/// Output:
/// - Test passes when auto-import suggests
///   `import pkg.generated.Widget.{render}.`.
///
/// Transformation:
/// - Locks binding-generator `.typi` summaries into the same auto-import path
///   as package-local provider summaries.
#[test]
fn import_candidate_inserts_selected_function_from_generated_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-generated-function-{}-{unique}",
        std::process::id()
    ));
    let summary_dir = temp_dir.join("std").join("summaries");
    fs::create_dir_all(&summary_dir)?;
    fs::write(
        summary_dir.join("pkg.generated.Widget.typi"),
        "\
module pkg.generated.Widget.

pub render(value: String): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(): String ->
  render(\"Ada\").
";

    let candidates = import_candidates_for_symbol(&uri, text, "render");
    let render = candidates
        .iter()
        .find(|candidate| candidate.title == "Import render from pkg.generated.Widget")
        .expect("generated render import candidate");
    let edit = candidate_edit(render);

    assert_eq!(edit.new_text, "import pkg.generated.Widget.{render}.\n");
    assert_eq!(edit.range.start, Position::new(2, 0));
    assert_eq!(edit.range.end, Position::new(2, 0));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies unknown-function diagnostics produce selected import edits.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2`.
/// - A consumer source that calls `add` without importing it.
/// - Stable unknown-function diagnostic spellings for `add/2`.
///
/// Output:
/// - Test passes when each LSP quick fix inserts `import math.{add}.`.
///
/// Transformation:
/// - Routes missing-function diagnostics through the same provider-summary
///   selected-import repair path as direct symbol import candidates.
#[test]
fn diagnostic_import_action_repairs_provider_function() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-function-diagnostic-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("math.terli"),
        "\
module math.

pub add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(): Int ->
  add(1, 2).
";

    for diagnostic in [
        "unknown function add / 2",
        "unknown function add/2",
        "unknown function math.add / 2",
    ] {
        let actions = import_code_actions_for_diagnostic(&uri, text, diagnostic);
        let action = actions
            .iter()
            .find(|action| action.title == "Import add from math")
            .unwrap_or_else(|| panic!("missing add action for {diagnostic}; actions: {actions:?}"));
        let edit = action.edit.as_ref().expect("workspace edit");
        let changes = edit.changes.as_ref().expect("workspace edit changes");
        let edits = changes.get(&uri).expect("edit for current uri");

        assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "import math.{add}.\n");
        assert_eq!(edits[0].range.start, Position::new(2, 0));
        assert_eq!(edits[0].range.end, Position::new(2, 0));
    }

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies selected imports from the same provider are grouped.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2` and `subtract/2`.
/// - A consumer source that already imports `subtract` from the provider.
///
/// Output:
/// - Test passes when the quick fix expands the existing selected import line
///   to include `add` instead of inserting a second import declaration.
///
/// Transformation:
/// - Keeps auto-import output aligned with formatter/lint grouping rules for
///   selected imports from the same module.
#[test]
fn import_candidate_groups_selected_function_with_existing_import() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-grouped-function-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("math.terli"),
        "\
module math.

pub add(left: Int, right: Int): Int.

pub subtract(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

import math.{subtract}.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");
    let add = candidates
        .iter()
        .find(|candidate| candidate.title == "Import add from math")
        .expect("grouped add import candidate");
    let edit = candidate_edit(add);

    assert_eq!(edit.new_text, "import math.{add, subtract}.\n");
    assert_eq!(edit.range.start, Position::new(2, 0));
    assert_eq!(edit.range.end.line, 3);
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies provider constructors produce selected imports.
///
/// Inputs:
/// - A sibling provider `.terli` whose module leaf differs from its exported
///   constructor name.
/// - A consumer source that calls the constructor without importing it.
///
/// Output:
/// - Test passes when auto-import suggests `import items.{Items}.` instead of
///   a bare `import items.` that would not expose the constructor name.
///
/// Transformation:
/// - Locks constructor repair to selected imports for provider-owned symbols.
#[test]
fn import_candidate_inserts_selected_constructor_from_provider_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-constructor-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("items.terli"),
        "\
module items.

pub type Items.

pub constructor Items {
    (values: List[Int]): Items ->
        terlan_interface_constructor
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(values: List[Int]): Items ->
  Items(values).
";

    let candidates = import_candidates_for_symbol(&uri, text, "Items");
    let items = candidates
        .iter()
        .find(|candidate| candidate.title == "Import Items from items")
        .expect("selected Items import candidate");
    let edit = candidate_edit(items);

    assert_eq!(edit.new_text, "import items.{Items}.\n");
    assert_eq!(edit.range.start, Position::new(2, 0));
    assert_eq!(edit.range.end, Position::new(2, 0));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies provider constructor diagnostics carry selected-import edits.
///
/// Inputs:
/// - A sibling provider `.terli` that exports constructor `Items`.
/// - A consumer source that calls `Items(values)` without importing it.
/// - Stable unknown-constructor diagnostic spellings for `Items / 1`.
///
/// Output:
/// - Test passes when the LSP code action is a quick fix whose workspace edit
///   inserts `import items.{Items}.`.
///
/// Transformation:
/// - Exercises provider-summary constructor repair through the actual LSP
///   `CodeAction` surface so editors can apply the selected import directly
///   for both spaced and compact arity spellings.
#[test]
fn diagnostic_import_action_contains_provider_constructor_workspace_edit() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-constructor-diagnostic-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("items.terli"),
        "\
module items.

pub type Items.

pub constructor Items {
    (values: List[Int]): Items ->
        terlan_interface_constructor
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub value(values: List[Int]): Items ->
  Items(values).
";

    for diagnostic in [
        "unknown constructor Items / 1",
        "unknown constructor Items/1",
    ] {
        let actions = import_code_actions_for_diagnostic(&uri, text, diagnostic);
        let action = actions
            .iter()
            .find(|action| action.title == "Import Items from items")
            .unwrap_or_else(|| {
                panic!("missing selected Items code action for {diagnostic}; actions: {actions:?}")
            });
        let edit = action.edit.as_ref().expect("workspace edit");
        let changes = edit.changes.as_ref().expect("workspace edit changes");
        let edits = changes.get(&uri).expect("edit for current uri");

        assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "import items.{Items}.\n");
        assert_eq!(edits[0].range.start, Position::new(2, 0));
        assert_eq!(edits[0].range.end, Position::new(2, 0));
    }
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies constructor-pattern diagnostics produce provider import edits.
///
/// Inputs:
/// - A sibling provider `.terli` that exports constructor `Items`.
/// - A consumer source that pattern matches on `Items(values)` without
///   importing it.
/// - Stable unknown-constructor-pattern diagnostic spellings for `Items`.
///
/// Output:
/// - Test passes when the LSP code action is a quick fix whose workspace edit
///   inserts `import items.{Items}.`.
///
/// Transformation:
/// - Extends the same auto-import repair path used for constructor calls to
///   constructor patterns, so pattern-matching diagnostics remain fixable for
///   both unarity and compact arity spellings.
#[test]
fn diagnostic_import_action_repairs_provider_constructor_pattern() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-constructor-pattern-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("items.terli"),
        "\
module items.

pub type Items.

pub constructor Items {
    (values: List[Int]): Items ->
        terlan_interface_constructor
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("sample.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let text = "\
module sample.

pub count(items: Items): Int ->
  case items {
    Items(values) -> 1
  }.
";

    for diagnostic in [
        "unknown constructor pattern Items",
        "unknown constructor pattern Items/1",
    ] {
        let actions = import_code_actions_for_diagnostic(&uri, text, diagnostic);
        let action = actions
            .iter()
            .find(|action| action.title == "Import Items from items")
            .unwrap_or_else(|| {
                panic!(
                    "missing selected Items pattern action for {diagnostic}; actions: {actions:?}"
                )
            });
        let edit = action.edit.as_ref().expect("workspace edit");
        let changes = edit.changes.as_ref().expect("workspace edit changes");
        let edits = changes.get(&uri).expect("edit for current uri");

        assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "import items.{Items}.\n");
        assert_eq!(edits[0].range.start, Position::new(2, 0));
        assert_eq!(edits[0].range.end, Position::new(2, 0));
    }
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
