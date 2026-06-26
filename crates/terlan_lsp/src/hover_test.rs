use std::fs;
use std::io::{self as std_io, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

use tower_lsp::lsp_types::{HoverContents, MarkupKind, Position, Url};

use super::hover_for_position;
use crate::document::{DocumentKind, OpenDocument};

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
    assert!(markup.value.contains("pub answer(): Int"));
    assert!(markup.value.contains("Returns the stable answer."));
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
    assert!(markup.value.contains("pub to_string(value: Int): String"));
    assert!(markup
        .value
        .contains("Converts the value to provider text."));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
