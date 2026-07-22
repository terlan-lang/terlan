use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

use tower_lsp::lsp_types::{CompletionItemKind, HoverContents, MarkupKind, Position, Url};

use super::{hover_for_position, Backend};
use crate::terlan_lsp::document::{DocumentKind, OpenDocument};

/// Verifies local declaration docs are served through hover.
///
/// Inputs:
/// - An open document containing a documented public function and a call.
///
/// Output:
/// - Test passes when hovering the call returns Markdown with the function
///   documentation and source-like signature.
///
/// Transformation:
/// - Exercises the LSP hover helper without JSON-RPC transport, proving local
///   syntax-output docs are packaged into editor hover content.
#[test]
fn hover_returns_same_document_function_docs() {
    let uri = Url::parse("file:///tmp/hover_local.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_local.

/**
 * Returns the stable answer.
 */
pub answer(): Int ->
  42.

pub caller(): Int ->
  answer().
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover = hover_for_position(&uri, &document, Position::new(9, 3)).expect("local hover docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(markup.value.contains("**function `answer`**"));
    assert!(markup.value.contains("@pure\npub answer(): Int"));
    assert!(markup.value.contains("Returns the stable answer."));
}

/// Verifies implication evidence remains visible in local editor hover.
#[test]
fn hover_preserves_local_structural_implication_signature() {
    let uri = Url::parse("file:///tmp/hover_implication_local.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_implication_local.

pub struct User { name: String }.

/// Reads the proven public name field.
pub display_name[T => {name: String}](value: T): String ->
  value.name.

pub caller(user: User): String ->
  display_name(user).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(9, 4)).expect("local implication hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup
        .value
        .contains("pub display_name[T => {name: String}](value: T): String"));
    assert!(markup.value.contains("Reads the proven public name field."));
}

/// Verifies imported implication functions remain discoverable and retain
/// their generated-summary evidence in hover and completion documentation.
#[test]
fn editor_surfaces_preserve_imported_structural_implication() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-hover-implication-import-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

/// Reads a field proven by structural implication evidence.
@pure
pub display_name[T => {name: String}](value: T): String.
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

import provider.{display_name}.

pub caller(): Unit ->
  display_name.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover = hover_for_position(&uri, &document, Position::new(5, 4))
        .expect("imported implication hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup
        .value
        .contains("pub display_name[T => {name: String}](value: T): String"));

    let items = Backend::completion_items_for_position(&uri, &document, Position::new(5, 2));
    let item = items
        .iter()
        .find(|item| item.label == "display_name")
        .expect("imported implication completion");
    assert_eq!(item.kind, Some(CompletionItemKind::FUNCTION));
    assert!(item
        .documentation
        .as_ref()
        .is_some_and(|docs| format!("{docs:?}").contains("structural implication evidence")));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[test]
fn hover_does_not_mark_inferred_effectful_function_pure() {
    let uri = Url::parse("file:///tmp/hover_impure_local.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "module hover_impure_local.\n\npub replace_first(values: List[Int]): Unit ->\n  values[0] = 1.\n\npub caller(values: List[Int]): Unit ->\n  replace_first(values).\n"
            .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover = hover_for_position(&uri, &document, Position::new(6, 3))
        .expect("effectful local hover docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(markup
        .value
        .contains("pub replace_first(values: List[Int]): Unit"));
    assert!(!markup.value.contains("@pure\npub replace_first"));
}

#[test]
fn value_lifecycle_hover_exposes_typed_constant_metadata() {
    let uri = Url::parse("file:///tmp/hover_constant.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "module hover_constant.\n\n/// Stable answer.\npub const ANSWER: Int = 42.\n\npub run(): Int ->\n  ANSWER.\n"
            .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(6, 3)).expect("constant hover metadata");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };
    assert!(
        markup.value.contains("**constant `ANSWER`**"),
        "{}",
        markup.value
    );
    assert!(markup.value.contains("pub const ANSWER: Int = 42"));
    assert!(markup.value.contains("Stable answer."));
}

#[test]
fn hover_returns_same_document_pure_trait_method_contract() {
    let uri = Url::parse("file:///tmp/hover_trait_pure.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_trait_pure.

pub trait Show[T] {
  @pure
  show(value: T): String.
}.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(4, 3)).expect("pure trait method hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(markup.value.contains("**trait method `show`**"));
    assert!(markup.value.contains("@pure\nshow(value: T): String"));
}

/// Verifies local type hover exposes negative trait facts.
#[test]
fn hover_returns_same_document_negative_trait_impls() {
    let uri = Url::parse("file:///tmp/hover_negative_trait.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_negative_trait.

/**
 * Secret material that must not be logged.
 */
pub type SecretKey = String.

pub trait Log[T] { log(value: T): String. }.
pub impl not Log[SecretKey].
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(5, 12)).expect("negative trait hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert!(markup.value.contains("**type `SecretKey`**"));
    assert!(markup
        .value
        .contains("Secret material that must not be logged."));
    assert!(markup.value.contains("**Negative trait implementations**"));
    assert!(markup.value.contains("pub impl not Log[SecretKey]."));
}

/// Verifies local hover renders function-head pattern parameters as source.
///
/// Inputs:
/// - An open document containing a documented function whose ABI parameter is
///   generated from tuple destructuring syntax.
///
/// Output:
/// - Test passes when hovering the call returns Markdown with the structural
///   parameter spelling and without the generated `_ArgN` name.
///
/// Transformation:
/// - Exercises syntax-output `pattern_text` through the LSP hover renderer so
///   editor documentation matches the Terlan source the user wrote.
#[test]
fn hover_renders_function_head_pattern_parameter_docs() {
    let uri = Url::parse("file:///tmp/hover_function_head_pattern.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_function_head_pattern.

/**
 * Adds a pair.
 */
pub add_pair({left, right}: Dynamic): Int ->
  left + right.

pub caller(): Int ->
  add_pair({1, 2}).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover = hover_for_position(&uri, &document, Position::new(9, 3))
        .expect("function-head pattern hover docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(markup.value.contains("**function `add_pair`**"));
    assert!(markup
        .value
        .contains("pub add_pair({left, right}: Dynamic): Int"));
    assert!(!markup.value.contains("_Arg1"));
    assert!(markup.value.contains("Adds a pair."));
}

/// Verifies local raw shape declaration docs are served through hover.
///
/// Inputs:
/// - An open document containing a documented parse-preserved `shape`
///   declaration.
///
/// Output:
/// - Test passes when hovering the shape declaration name returns Markdown with
///   the shape documentation and raw source-like signature.
///
/// Transformation:
/// - Keeps the editor hover contract useful before semantic shape expansion is
///   implemented by treating raw shape declarations as documentable syntax.
#[test]
fn hover_returns_same_document_raw_shape_docs() {
    let uri = Url::parse("file:///tmp/hover_shape.terl").expect("uri");
    let document = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "\
module hover_shape.

/**
 * Matches a user asset path.
 */
pub shape UserAsset(id, file) =
  \"users/${id: Int}/assets/${file}\".

pub value(): Int ->
  1.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(5, 12)).expect("shape hover docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert!(markup.value.contains("**shape `UserAsset`**"));
    assert!(markup.value.contains("pub shape UserAsset"));
    assert!(markup.value.contains("\"users/${id: Int}/assets/${file}\""));
    assert!(markup.value.contains("Matches a user asset path."));
}

/// Verifies imported interface docs are served through hover.
///
/// Inputs:
/// - A temporary provider `.terli` interface containing function docs.
/// - A consumer source file importing and calling that function.
///
/// Output:
/// - Test passes when hovering the imported call returns provider docs.
///
/// Transformation:
/// - Uses the same file-set interface loading path as diagnostics/import
///   actions so packaged `.typi`/`.terli` docs become editor hover content.
#[test]
fn hover_returns_imported_function_docs_from_interface() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-hover-import-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
//! Provider module docs.
module provider.

/// Converts the value to provider text.
@pure
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

pub caller(): String ->
  to_string(1).
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(5, 4)).expect("imported hover docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert!(markup.value.contains("**function `provider.to_string`**"));
    assert!(markup
        .value
        .contains("@pure\npub to_string(value: Int): String"));
    assert!(markup
        .value
        .contains("Converts the value to provider text."));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies imported type hover exposes only exported negative trait facts.
#[test]
fn hover_returns_imported_negative_trait_impls_from_interface() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-hover-negative-trait-import-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
module provider.

/// Secret material owned by the provider.
pub type SecretKey = String.
pub trait Log[T] { log(value: T): String. }.
pub trait Compare[T] { compare(left: T, right: T): Int. }.
pub impl not Log[SecretKey].
impl not Compare[SecretKey].
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

import type provider.{SecretKey}.

pub expose(value: SecretKey): SecretKey -> value.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover = hover_for_position(&uri, &document, Position::new(2, 24))
        .expect("imported negative trait hover");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert!(markup.value.contains("**type `provider.SecretKey`**"));
    assert!(markup.value.contains("pub impl not Log[SecretKey]."));
    assert!(!markup.value.contains("Compare"));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies imported interface shape docs are served through hover.
///
/// Inputs:
/// - A temporary provider `.terli` interface containing a documented public
///   shape declaration.
/// - A consumer source file importing that shape name.
///
/// Output:
/// - Test passes when hovering the imported shape name returns provider docs.
///
/// Transformation:
/// - Exercises summary-backed shape metadata through the same hover path used
///   for imported types and functions.
#[test]
fn hover_returns_imported_shape_docs_from_interface() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-hover-shape-import-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("provider.terli"),
        "\
//! Provider module docs.
module provider.

/// Matches a user asset path.
pub shape UserAsset(id, file) =
  \"users/${id: Int}/assets/${file}\".
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

import provider.{UserAsset}.

pub caller(): Int ->
  1.
"
        .to_string(),
        parse_ok: true,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };

    let hover =
        hover_for_position(&uri, &document, Position::new(2, 19)).expect("imported shape docs");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("expected markdown hover");
    };

    assert!(markup.value.contains("**shape `provider.UserAsset`**"));
    assert!(markup.value.contains("pub shape UserAsset"));
    assert!(markup.value.contains("\"users/${id: Int}/assets/${file}\""));
    assert!(markup.value.contains("Matches a user asset path."));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
