use super::*;

/// Parsed Terlan struct or record field key.
///
/// Inputs:
/// - A lower-case field name, optionally prefixed with `#`.
///
/// Output:
/// - Field name text without the private marker, plus a boolean private flag.
///
/// Transformation:
/// - Keeps source-level privacy syntax out of the stored field identifier while
///   preserving whether the field was written with the private `#` marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParsedFieldKey {
    pub(super) name: String,
    pub(super) is_private: bool,
}

impl Parser {
    /// Decodes a map field key into its runtime string identity.
    ///
    /// Inputs:
    /// - `token`: lower-case identifier or quoted string accepted as a map key.
    ///
    /// Output:
    /// - The identifier spelling or decoded string payload.
    ///
    /// Transformation:
    /// - Removes string delimiters and decodes escapes once at the parser
    ///   boundary so map expressions, patterns, and runtime string keys share
    ///   one canonical identity.
    pub(super) fn map_field_key_text(token: &Token) -> Option<String> {
        match token.kind {
            TokenKind::Atom => Some(token.text.clone()),
            TokenKind::String => parse_string_token_payload(&token.text),
            _ => None,
        }
    }

    /// Parses a struct or record field key.
    ///
    /// Inputs:
    /// - Parser cursor positioned at either `LowerIdent` or `# LowerIdent`.
    /// - `message`: diagnostic text used when the lower-case field name is
    ///   absent.
    ///
    /// Output:
    /// - A `ParsedFieldKey` containing the clean field name and privacy flag.
    ///
    /// Transformation:
    /// - Consumes the optional private marker and the required lower-case
    ///   identifier without admitting `#` syntax into map-field positions.
    pub(super) fn parse_record_field_key(&mut self, message: &str) -> ParseResult<ParsedFieldKey> {
        let is_private = self.consume_if(TokenKind::Hash);
        let name = self.expect_lower_ident(message)?;
        Ok(ParsedFieldKey { name, is_private })
    }

    /// Formats a parsed field key back into source-like field text.
    ///
    /// Inputs:
    /// - `field`: parsed field key from struct or record syntax.
    ///
    /// Output:
    /// - `field` for public fields and `#field` for private fields.
    ///
    /// Transformation:
    /// - Reintroduces the private marker only when it was present in source.
    pub(super) fn field_key_text(field: &ParsedFieldKey) -> String {
        if field.is_private {
            format!("#{}", field.name)
        } else {
            field.name.clone()
        }
    }
}
