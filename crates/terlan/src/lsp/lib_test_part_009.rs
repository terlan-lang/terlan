
/// Verifies private provider symbols are not navigation targets.
///
/// Inputs:
/// - A provider source file containing private function `secret/0`.
/// - A sibling public `.terli` summary that does not export `secret/0`.
/// - A consumer document attempting to selected-import and call `secret`.
///
/// Output:
/// - Test passes when go-to-definition returns no provider location.
///
/// Transformation:
/// - Enforces provider-summary visibility as the navigation boundary so editor
///   definition lookup does not expose private implementation details.
#[test]
fn definition_locations_reject_private_provider_symbol() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-private-provider-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

pub public_value(): Int.
",
    )?;
    fs::write(
        temp_dir.join("provider.terl"),
        "\
module provider.

secret(): Int ->
  1.

pub public_value(): Int ->
  secret().
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{secret}.

pub caller(): Int ->
  secret().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert!(
        locations.is_empty(),
        "private symbol locations: {locations:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies missing provider files do not produce stale definition targets.
///
/// Inputs:
/// - A consumer document importing `missing.{value}`.
/// - No sibling source, interface, or summary file for module `missing`.
///
/// Output:
/// - Test passes when go-to-definition returns no location.
///
/// Transformation:
/// - Locks cross-file definition lookup to existing provider artifacts instead
///   of returning stale paths or guessed source ranges for unavailable modules.
#[test]
fn definition_locations_reject_missing_provider_file() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-missing-provider-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import missing.{value}.

pub caller(): Int ->
  value().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert!(
        locations.is_empty(),
        "missing provider locations: {locations:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies local definitions take precedence over imported symbols.
///
/// Inputs:
/// - A sibling provider `.terli` exporting `to_string/1`.
/// - A consumer document importing that provider symbol and declaring a local
///   `to_string/1` with the same name.
///
/// Output:
/// - Test passes when go-to-definition on the call resolves to the local
///   definition range, not the provider summary.
///
/// Transformation:
/// - Locks same-document symbol precedence before provider fallback lookup so
///   editor navigation follows the source binding developers see locally.
#[test]
fn definition_locations_prefer_local_definition_over_import() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-local-shadow-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

pub to_string(value: Int): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{to_string}.

pub to_string(value: Int): String ->
  \"local\".

pub caller(): String ->
  to_string(1).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(8, 4));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, uri);
    assert_eq!(locations[0].range.start, Position::new(4, 4));
    assert_eq!(locations[0].range.end, Position::new(4, 13));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies dotted package imports resolve through nested provider files.
///
/// Inputs:
/// - A provider summary at `pkg/math.terli` declaring module `pkg.math`.
/// - A consumer document importing `pkg.math.{add}` and calling `add`.
///
/// Output:
/// - Test passes when go-to-definition resolves to the nested provider summary.
///
/// Transformation:
/// - Exercises dotted-module to slash-path lookup for package-local provider
///   summaries without relying on flat generated-summary filenames.
#[test]
fn definition_locations_resolve_nested_package_provider_function() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-nested-provider-definition-{}-{unique}",
        std::process::id()
    ));
    let provider_dir = temp_dir.join("pkg");
    fs::create_dir_all(&provider_dir)?;
    let provider_path = provider_dir.join("math.terli");
    fs::write(
        &provider_path,
        "\
module pkg.math.

pub add(left: Int, right: Int): Int.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import pkg.math.{add}.

pub caller(): Int ->
  add(1, 2).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 7));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves selected imports backed by overload
/// summaries.
///
/// Inputs:
/// - A sibling provider `.terli` containing two public `pick/1` overloads.
/// - A consumer document importing `provider.{pick}` and calling `pick`.
///
/// Output:
/// - Test passes when the cursor on the imported call resolves to the provider
///   interface declaration range.
///
/// Transformation:
/// - Locks editor navigation to `ModuleInterface.function_overloads` visibility
///   so same-name same-arity public overloads remain navigable through selected
///   imports.
#[test]
fn definition_locations_resolve_imported_overloaded_function() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-overloaded-function-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub pick(value: Int): Int.
pub pick(value: String): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{pick}.

pub caller(): String ->
  pick(\"Ada\").
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 8));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition follows public selected re-export summaries.
///
/// Inputs:
/// - A base provider summary declaring public function `add/2`.
/// - A wrapper provider summary importing `base.{add}` and exporting `add/2`.
/// - A consumer importing `wrapper.{add}` and calling `add`.
///
/// Output:
/// - Test passes when the cursor on the wrapper import use resolves to the
///   original base provider declaration range.
///
/// Transformation:
/// - Locks LSP cross-file navigation to explicit provider-summary re-exports
///   instead of stopping at wrapper summaries that do not own the declaration.
#[test]
fn definition_locations_follow_selected_reexport_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-reexported-provider-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let base_path = temp_dir.join("base.terli");
    fs::write(
        &base_path,
        "\
module base.

pub add(left: Int, right: Int): Int.
",
    )?;
    fs::write(
        temp_dir.join("wrapper.terli"),
        "\
module wrapper.

import base.{add}.

export add/2.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let base_uri = Url::from_file_path(base_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid base URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import wrapper.{add}.

pub caller(): Int ->
  add(1, 2).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, base_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 7));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies stale re-export summaries do not produce provider locations.
///
/// Inputs:
/// - A wrapper provider summary importing `renamed.{add}` and exporting `add/2`.
/// - No `renamed` provider source, interface, or generated summary file.
/// - A consumer importing `wrapper.{add}` and calling `add`.
///
/// Output:
/// - Test passes when go-to-definition returns no location.
///
/// Transformation:
/// - Locks package-cache and renamed-file behavior so editor navigation does
///   not return guessed or stale targets when re-export metadata outlives the
///   original provider artifact.
#[test]
fn definition_locations_reject_stale_reexport_provider_summary() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-stale-reexport-provider-definition-{}-{unique}",
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
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import wrapper.{add}.

pub caller(): Int ->
  add(1, 2).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert!(
        locations.is_empty(),
        "stale re-export provider locations: {locations:?}"
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves generated summary function bindings.
///
/// Inputs:
/// - A generated-style `std/summaries/pkg.generated.Widget.typi` provider
///   summary with public function `render/1`.
/// - A consumer document importing `pkg.generated.Widget.{render}` and calling
///   the imported function.
///
/// Output:
/// - Test passes when the cursor on the imported function call resolves to the
///   generated summary declaration range.
///
/// Transformation:
/// - Extends generated summary navigation coverage from type declarations to
///   callable bindings emitted by binding generators.
#[test]
fn definition_locations_resolve_generated_summary_function_binding() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-generated-summary-function-definition-{}-{unique}",
        std::process::id()
    ));
    let summary_dir = temp_dir.join("std").join("summaries");
    fs::create_dir_all(&summary_dir)?;
    let provider_path = summary_dir.join("pkg.generated.Widget.typi");
    fs::write(
        &provider_path,
        "\
module pkg.generated.Widget.

pub render(value: String): String.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import pkg.generated.Widget.{render}.

pub caller(): String ->
  render(\"Ada\").
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 4));
    assert_eq!(locations[0].range.end, Position::new(2, 10));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves generated std summary provider files.
///
/// Inputs:
/// - A generated-style `std/summaries/std.core.Option.typi` provider summary.
/// - A consumer document importing `std.core.Option.{Option}` and referencing
///   the imported type.
///
/// Output:
/// - Test passes when the cursor on the imported type annotation resolves to
///   the generated summary declaration range.
///
/// Transformation:
/// - Exercises the deterministic generated-summary fallback path used for
///   packaged stdlib interfaces without depending on the real checked-in
///   summaries.
#[test]
fn definition_locations_resolve_generated_std_summary_type() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-generated-std-summary-definition-{}-{unique}",
        std::process::id()
    ));
    let summary_dir = temp_dir.join("std").join("summaries");
    fs::create_dir_all(&summary_dir)?;
    let provider_path = summary_dir.join("std.core.Option.typi");
    fs::write(
        &provider_path,
        "\
module std.core.Option.

pub type Option[T].
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import type std.core.Option.{Option}.

pub id(value: Option[Int]): Option[Int] ->
  value.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(4, 14));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 9));
    assert_eq!(locations[0].range.end, Position::new(2, 15));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves imported type references.
///
/// Inputs:
/// - A sibling provider `.terli` with a public bodyless type declaration.
/// - A consumer document importing that type and using it in annotations.
///
/// Output:
/// - Test passes when the cursor on the imported type annotation resolves to
///   the provider interface declaration range.
///
/// Transformation:
/// - Extends editor definition navigation beyond functions while still using
///   provider summaries as the public source of truth.
#[test]
fn definition_locations_resolve_imported_type_annotation() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-type-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub type ExternalUser.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import type provider.{ExternalUser}.

pub id(value: ExternalUser): ExternalUser ->
  value.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(4, 15));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 9));
    assert_eq!(locations[0].range.end, Position::new(2, 21));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves imported public struct fields.
///
/// Inputs:
/// - A sibling provider `.terli` with a public struct containing public and
///   private fields.
/// - A consumer document importing the struct type and accessing fields through
///   a typed receiver parameter.
///
/// Output:
/// - Test passes when the cursor on the public receiver field resolves to the
///   provider field declaration and the private field returns no target.
///
/// Transformation:
/// - Extends editor definition navigation to imported field members while
///   preserving provider-summary visibility boundaries.
#[test]
fn definition_locations_resolve_imported_struct_field_reference() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-field-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub struct ExternalUser {
    name: String,
    #secret: String
}.
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import type provider.{ExternalUser}.

pub user_name(user: ExternalUser): String ->
  user.name.

pub user_secret(user: ExternalUser): String ->
  user.secret.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 8));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(3, 4));
    assert_eq!(locations[0].range.end, Position::new(3, 8));

    let private_locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(8, 8));
    assert!(private_locations.is_empty());

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves imported shape references.
///
/// Inputs:
/// - A sibling provider `.terli` with a public raw shape declaration.
/// - A consumer document importing and referencing that shape name.
///
/// Output:
/// - Test passes when the cursor on the imported shape reference resolves to
///   the provider interface declaration range.
///
/// Transformation:
/// - Extends editor definition navigation to the reserved shape surface without
///   enabling shape expansion semantics.
#[test]
fn definition_locations_resolve_imported_shape_reference() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-shape-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub shape UserAsset(id) = \"users/${id}/asset\".
",
    )?;
    let uri = Url::from_file_path(temp_dir.join("consumer.terl"))
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid temp URI"))?;
    let provider_uri = Url::from_file_path(provider_path)
        .map_err(|()| std_io::Error::new(ErrorKind::InvalidInput, "invalid provider URI"))?;
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module consumer.

import provider.{UserAsset}.

pub route_name(): String ->
  UserAsset.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let locations =
        Backend::definition_locations_for_position(&uri, &document, Position::new(5, 3));

    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].uri, provider_uri);
    assert_eq!(locations[0].range.start, Position::new(2, 10));
    assert_eq!(locations[0].range.end, Position::new(2, 19));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
