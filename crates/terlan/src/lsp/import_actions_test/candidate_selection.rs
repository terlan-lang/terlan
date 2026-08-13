use super::*;

/// Verifies std constructor diagnostics produce selected-import fallbacks.
///
/// Inputs:
/// - Stable unknown-constructor diagnostics for std constructors whose names do
///   not match their module leaves.
/// - Source documents missing the relevant std imports.
///
/// Output:
/// - Test passes when each LSP quick fix inserts a selected import from the
///   owning std module.
///
/// Transformation:
/// - Exercises built-in selected-import fallbacks for installed compilers whose
///   std interface summaries are not available under the edited workspace.
#[test]
pub(super) fn diagnostic_import_action_inserts_std_constructor_selected_fallbacks() {
    let cases = [
        (
            "Some",
            "unknown constructor Some / 1",
            "Import Some from std.core.Option",
            "import std.core.Option.{Some}.\n",
        ),
        (
            "Some",
            "unknown constructor std.core.Option.Some / 1",
            "Import Some from std.core.Option",
            "import std.core.Option.{Some}.\n",
        ),
        (
            "None",
            "unknown constructor pattern None",
            "Import None from std.core.Option",
            "import std.core.Option.{None}.\n",
        ),
        (
            "Ok",
            "unknown constructor Ok / 1",
            "Import Ok from std.core.Result",
            "import std.core.Result.{Ok}.\n",
        ),
        (
            "Err",
            "unknown constructor Err / 1",
            "Import Err from std.core.Result",
            "import std.core.Result.{Err}.\n",
        ),
    ];

    for (symbol, diagnostic, title, import_text) in cases {
        let uri = test_uri();
        let text = format!("module sample.\n\npub value(): Dynamic ->\n  {symbol}(1).\n");
        let actions = import_code_actions_for_diagnostic(&uri, &text, diagnostic);
        let action = actions
            .iter()
            .find(|action| action.title == title)
            .unwrap_or_else(|| panic!("missing action {title}; actions: {actions:?}"));
        let edit = action.edit.as_ref().expect("workspace edit");
        let changes = edit.changes.as_ref().expect("workspace edit changes");
        let edits = changes.get(&uri).expect("edit for current uri");

        assert_eq!(action.kind.as_ref(), Some(&CodeActionKind::QUICKFIX));
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, import_text);
        assert_eq!(edits[0].range.start, Position::new(2, 0));
        assert_eq!(edits[0].range.end, Position::new(2, 0));
    }
}

/// Verifies existing selected imports suppress duplicate quick fixes.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2`.
/// - A consumer source that already imports `math.{add}`.
///
/// Output:
/// - Test passes when no selected `add` import candidate is produced.
///
/// Transformation:
/// - Locks auto-import to avoid duplicating selected imports while still
///   allowing other unresolved symbols to be repaired.
#[test]
pub(super) fn import_candidate_skips_already_selected_function_import() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-existing-function-{}-{unique}",
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

import math.{add}.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");

    assert!(
        candidates.is_empty(),
        "already selected add import should not produce candidates: {candidates:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies wildcard selected imports suppress duplicate quick fixes.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2`.
/// - A consumer source that already imports `math.{*}`.
///
/// Output:
/// - Test passes when no selected `add` import candidate is produced.
///
/// Transformation:
/// - Treats wildcard selected imports as exposing the provider's public names so
///   auto-import does not add redundant explicit selected imports.
#[test]
pub(super) fn import_candidate_skips_function_import_when_wildcard_selected() -> std_io::Result<()>
{
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-wildcard-function-{}-{unique}",
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

import math.{*}.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");

    assert!(
        candidates.is_empty(),
        "wildcard selected add import should not produce candidates: {candidates:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies selected import aliases do not hide unresolved source names.
///
/// Inputs:
/// - A sibling provider `.terli` that exports `add/2`.
/// - A consumer source that imports `add as plus` but still calls `add`.
///
/// Output:
/// - Test passes when auto-import still suggests the visible `add` import.
///
/// Transformation:
/// - Distinguishes provider source names from local alias names so an aliased
///   import suppresses quick fixes only for the alias it actually introduces.
#[test]
pub(super) fn import_candidate_keeps_source_name_candidate_when_existing_import_uses_alias(
) -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-aliased-function-{}-{unique}",
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

import math.{add as plus}.

pub value(): Int ->
  add(1, 2).
";

    let candidates = import_candidates_for_symbol(&uri, text, "add");

    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.title == "Import add from math"),
        "aliased add import should not hide source-name candidate: {candidates:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies ambiguous public function names produce explicit choices.
///
/// Inputs:
/// - Two sibling provider `.terli` files that both export `add/2`.
/// - A consumer source that calls `add` without importing it.
///
/// Output:
/// - Test passes when auto-import returns one selected-import candidate per
///   provider module, preserving module provenance in the action titles.
///
/// Transformation:
/// - Locks ambiguous auto-import behavior to ranked choices instead of picking
///   one provider silently.
#[test]
pub(super) fn import_candidate_keeps_ambiguous_function_choices() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-import-action-ambiguous-function-{}-{unique}",
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
    fs::write(
        temp_dir.join("stats.terli"),
        "\
module stats.

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
    let titles = candidates
        .iter()
        .map(|candidate| candidate.title.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        titles,
        vec!["Import add from math", "Import add from stats"]
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
