use super::support::*;

/// Verifies go-to-definition resolves imported constructor references.
///
/// Inputs:
/// - A sibling provider `.terli` with a public constructor declaration.
/// - A consumer document importing and calling that constructor.
///
/// Output:
/// - Test passes when the cursor on the imported constructor call resolves to
///   the provider constructor declaration range.
///
/// Transformation:
/// - Extends editor definition navigation to constructor imports without
///   conflating them with type declarations of the same module.
#[test]
pub(super) fn definition_locations_resolve_imported_constructor_reference() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-constructor-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub type ExternalUser.

pub constructor BuildExternalUser {
    (name: String): ExternalUser ->
        terlan_interface_constructor
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

import provider.{BuildExternalUser}.

pub made(): Dynamic ->
  BuildExternalUser(\"Ada\").
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
    assert_eq!(locations[0].range.start, Position::new(4, 16));
    assert_eq!(locations[0].range.end, Position::new(4, 33));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies go-to-definition resolves imported trait references.
///
/// Inputs:
/// - A sibling provider `.terli` with a public trait declaration.
/// - A consumer document importing and referencing that trait name.
///
/// Output:
/// - Test passes when the cursor on the imported trait reference resolves to
///   the provider trait declaration range.
///
/// Transformation:
/// - Extends editor definition navigation to public trait summaries, keeping
///   provider interfaces as the public source of truth.
#[test]
pub(super) fn definition_locations_resolve_imported_trait_reference() -> std_io::Result<()> {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!(
        "terlan-lsp-imported-trait-definition-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir)?;
    let provider_path = temp_dir.join("provider.terli");
    fs::write(
        &provider_path,
        "\
module provider.

pub trait Named[T] {
    name(value: T): String.
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

import provider.{Named}.

pub trait_name(): Dynamic ->
  Named.
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
    assert_eq!(locations[0].range.end, Position::new(2, 15));
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

/// Verifies identifier extraction stays conservative.
///
/// Inputs:
/// - A source snippet with identifiers and punctuation.
///
/// Output:
/// - Test passes when identifier offsets return names and punctuation offsets
///   return no result.
///
/// Transformation:
/// - Locks the ASCII identifier subset used by the current same-file
///   definition provider.
#[test]
pub(super) fn identifier_at_byte_offset_extracts_ascii_identifier() {
    let text = "target().";

    assert_eq!(
        Backend::identifier_at_byte_offset(text, 2),
        Some("target".to_string())
    );
    assert_eq!(Backend::identifier_at_byte_offset(text, 7), None);
}

#[test]
pub(super) fn open_document_position_to_byte_offset() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "a😀\nxy".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 0)), Some(0));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 1)), Some(1));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 3)), Some(5));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 0)), Some(6));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 2)), Some(8));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 7)), None);
}

#[test]
pub(super) fn open_document_position_to_byte_offset_with_crlf() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "a😀\r\nb\n".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 0)), Some(0));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 1)), Some(1));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 3)), Some(5));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 0)), Some(7));
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 1)), Some(8));
    assert_eq!(doc.byte_offset_from_position(Position::new(2, 0)), Some(9));
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 2)), None);
}

#[test]
pub(super) fn open_document_position_to_byte_offset_invalid_inputs() {
    let doc = OpenDocument {
        version: 1,
        language_id: "terlan".to_string(),
        kind: DocumentKind::Source,
        text: "hello\nworld".to_string(),
        parse_ok: false,
        resolve_diagnostics: Vec::new(),
        type_diagnostics: Vec::new(),
        template_diagnostics: Vec::new(),
    };
    assert_eq!(doc.byte_offset_from_position(Position::new(5, 0)), None);
    assert_eq!(doc.byte_offset_from_position(Position::new(1, 99)), None);
    assert_eq!(doc.byte_offset_from_position(Position::new(0, 6)), None);
}

#[test]
pub(super) fn open_document_range_from_span_uses_utf16_positions() {
    let text = "module emoji.\n\npub value(): Text ->\n    \"a😀b\".\n";
    let start = text.find('😀').expect("emoji offset");
    let end = start + '😀'.len_utf8();

    let range = OpenDocument::range_from_span(text, &Span::new(start, end));

    assert_eq!(range.start, Position::new(3, 6));
    assert_eq!(range.end, Position::new(3, 8));
}

#[test]
pub(super) fn open_document_range_from_span_handles_crlf() {
    let text = "module crlf.\r\n\r\npub value(): Int ->\r\n    1.\r\n";
    let start = text.find('1').expect("number offset");
    let end = start + 1;

    let range = OpenDocument::range_from_span(text, &Span::new(start, end));

    assert_eq!(range.start, Position::new(3, 4));
    assert_eq!(range.end, Position::new(3, 5));
}
