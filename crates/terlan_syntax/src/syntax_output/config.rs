use super::*;

/// Returns whether a preserved raw declaration is a canonical config declaration.
///
/// Inputs:
/// - `kind`: parser-preserved declaration head.
///
/// Output:
/// - `true` when the declaration belongs to the EBNF `ConfigDecl` family.
///
/// Transformation:
/// - Keeps the private parse tree unchanged while allowing syntax output to expose the
///   formal config declaration payload instead of a raw placeholder.
pub(super) fn is_config_declaration_kind(kind: &str) -> bool {
    matches!(kind, "target" | "native" | "machine" | "static")
}

/// Extracts the config declaration target from preserved declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text, such as
///   `target erlang` or `target js { module: true }`.
///
/// Output:
/// - The first target segment after the config declaration name, or an empty
///   string when preserved declaration text is malformed.
///
/// Transformation:
/// - Reads only the declaration head and leaves the full metadata body in
///   `text`, so later structured metadata parsing can replace this shim without
///   changing consumers that only need the target.

/// Extracts the config declaration target from preserved declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text, such as
///   `target erlang` or `target js { module: true }`.
///
/// Output:
/// - The first target segment after the config declaration name, or an empty
///   string when preserved declaration text is malformed.
///
/// Transformation:
/// - Reads only the declaration head and leaves the full metadata body in
///   `text`, so later structured metadata parsing can replace this shim without
///   changing consumers that only need the target.
pub(super) fn config_declaration_target(text: &str) -> String {
    text.split_whitespace()
        .nth(1)
        .map(|part| {
            part.trim_matches(|ch: char| matches!(ch, '{' | '}' | '.' | ',' | ';'))
                .to_string()
        })
        .unwrap_or_default()
}

/// Parses structured config entries from preserved config declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text.
///
/// Output:
/// - Structured config entries when the text follows `ConfigDecl` metadata
///   block syntax.
/// - An empty entry list for empty blocks, blockless declarations, lexer
///   errors, or raw declarations outside the formal config shape.
///
/// Transformation:
/// - Re-lexes the preserved text, skips the config name and target path, and
///   parses a metadata block only when it appears immediately after the target.
///   This keeps target-specific semantics explicit while making the syntax
///   contract structured enough for validators and phase manifests.

/// Parses structured config entries from preserved config declaration text.
///
/// Inputs:
/// - `text`: parser-preserved config declaration text.
///
/// Output:
/// - Structured config entries when the text follows `ConfigDecl` metadata
///   block syntax.
/// - An empty entry list for empty blocks, blockless declarations, lexer
///   errors, or raw declarations outside the formal config shape.
///
/// Transformation:
/// - Re-lexes the preserved text, skips the config name and target path, and
///   parses a metadata block only when it appears immediately after the target.
///   This keeps target-specific semantics explicit while making the syntax
///   contract structured enough for validators and phase manifests.
pub(super) fn parse_config_entries(text: &str) -> Vec<SyntaxConfigEntryOutput> {
    let Ok(tokens) = lex(text) else {
        return Vec::new();
    };
    let mut parser = ConfigEntryParser {
        tokens: &tokens,
        pos: 0,
    };
    parser.parse_entries().unwrap_or_default()
}

struct ConfigEntryParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl ConfigEntryParser<'_> {
    /// Parses a complete config declaration into metadata entries.
    ///
    /// Inputs:
    /// - `self`: parser cursor over tokens from preserved config text.
    ///
    /// Output:
    /// - Parsed metadata entries, or `None` when the text is not the formal
    ///   `ConfigName ConfigTarget MetadataBlock` shape.
    ///
    /// Transformation:
    /// - Consumes the declaration head, target path, and immediate metadata
    ///   block. Blockless declarations and non-immediate blocks are treated as
    ///   entryless preserved config text.
    fn parse_entries(&mut self) -> Option<Vec<SyntaxConfigEntryOutput>> {
        self.expect_identifier()?;
        self.parse_config_path()?;
        if !self.consume(TokenKind::LBrace) {
            return Some(Vec::new());
        }
        self.parse_entry_list(TokenKind::RBrace, TokenKind::Semicolon)
    }

    /// Parses a dot-qualified config path.
    ///
    /// Inputs:
    /// - Cursor positioned at the first path segment.
    ///
    /// Output:
    /// - `Some(())` when at least one identifier segment was consumed.
    ///
    /// Transformation:
    /// - Consumes `LowerIdent { "." LowerIdent }` in the token stream used by
    ///   config targets and keys.
    fn parse_config_path(&mut self) -> Option<()> {
        self.expect_identifier()?;
        while self.consume(TokenKind::Dot) {
            self.expect_identifier()?;
        }
        Some(())
    }

    /// Parses metadata entries until a closing delimiter.
    ///
    /// Inputs:
    /// - `close`: token that terminates the current metadata container.
    /// - `separator`: token separating entries.
    ///
    /// Output:
    /// - Entry list when every entry parses and the closing token is present.
    ///
    /// Transformation:
    /// - Accepts optional trailing separators and delegates value parsing to
    ///   `parse_value`.
    fn parse_entry_list(
        &mut self,
        close: TokenKind,
        separator: TokenKind,
    ) -> Option<Vec<SyntaxConfigEntryOutput>> {
        let mut entries = Vec::new();
        if self.consume(close.clone()) {
            return Some(entries);
        }
        loop {
            entries.push(self.parse_entry()?);
            if self.consume(separator.clone()) {
                if self.consume(close.clone()) {
                    break;
                }
                continue;
            }
            self.expect(close.clone())?;
            break;
        }
        Some(entries)
    }

    /// Parses one key/value metadata entry.
    ///
    /// Inputs:
    /// - Cursor positioned at `ConfigKey`.
    ///
    /// Output:
    /// - Parsed entry with dotted key text and typed value.
    ///
    /// Transformation:
    /// - Reads the key, consumes `:`, and parses the value using the formal
    ///   config value grammar instead of Terlan runtime expression parsing.
    fn parse_entry(&mut self) -> Option<SyntaxConfigEntryOutput> {
        let key = self.parse_key()?;
        self.expect(TokenKind::Colon)?;
        let value = self.parse_value()?;
        Some(SyntaxConfigEntryOutput { key, value })
    }

    /// Parses a dotted config key.
    ///
    /// Inputs:
    /// - Cursor positioned at the first key segment.
    ///
    /// Output:
    /// - Dotted key string.
    ///
    /// Transformation:
    /// - Reconstructs `LowerIdent { "." LowerIdent }` without preserving
    ///   whitespace because config keys are semantic identifiers.
    fn parse_key(&mut self) -> Option<String> {
        let mut key = self.expect_identifier()?;
        while self.consume(TokenKind::Dot) {
            key.push('.');
            key.push_str(&self.expect_identifier()?);
        }
        Some(key)
    }

    /// Parses one typed config value.
    ///
    /// Inputs:
    /// - Cursor positioned at the value token.
    ///
    /// Output:
    /// - Structured config value when the token sequence matches the config
    ///   value grammar.
    ///
    /// Transformation:
    /// - Classifies booleans, symbols, numbers, strings, lists, and maps
    ///   independently from runtime Terlan expressions.
    fn parse_value(&mut self) -> Option<SyntaxConfigValueOutput> {
        let token = self.current()?;
        let kind = token.kind.clone();
        let text = token.text.clone();
        match kind {
            TokenKind::Atom if text == "true" => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Bool { value: true })
            }
            TokenKind::Atom if text == "false" => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Bool { value: false })
            }
            TokenKind::Atom => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Symbol { value: text })
            }
            TokenKind::Int => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Int { value: text })
            }
            TokenKind::Float => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::Float { value: text })
            }
            TokenKind::String => {
                self.pos += 1;
                Some(SyntaxConfigValueOutput::String { value: text })
            }
            TokenKind::LBracket => self.parse_list(),
            TokenKind::Hash => self.parse_map(),
            _ => None,
        }
    }

    /// Parses a config list value.
    ///
    /// Inputs:
    /// - Cursor positioned at `[`.
    ///
    /// Output:
    /// - Structured list value.
    ///
    /// Transformation:
    /// - Parses comma-separated config values and accepts an empty list or a
    ///   trailing comma.
    fn parse_list(&mut self) -> Option<SyntaxConfigValueOutput> {
        self.expect(TokenKind::LBracket)?;
        let mut values = Vec::new();
        if self.consume(TokenKind::RBracket) {
            return Some(SyntaxConfigValueOutput::List { values });
        }
        loop {
            values.push(self.parse_value()?);
            if self.consume(TokenKind::Comma) {
                if self.consume(TokenKind::RBracket) {
                    break;
                }
                continue;
            }
            self.expect(TokenKind::RBracket)?;
            break;
        }
        Some(SyntaxConfigValueOutput::List { values })
    }

    /// Parses a config map value.
    ///
    /// Inputs:
    /// - Cursor positioned at `#`.
    ///
    /// Output:
    /// - Structured map value.
    ///
    /// Transformation:
    /// - Consumes `#{ ... }` and parses comma-separated config map entries
    ///   using the same entry shape as top-level metadata blocks.
    fn parse_map(&mut self) -> Option<SyntaxConfigValueOutput> {
        self.expect(TokenKind::Hash)?;
        self.expect(TokenKind::LBrace)?;
        let entries = self.parse_entry_list(TokenKind::RBrace, TokenKind::Comma)?;
        Some(SyntaxConfigValueOutput::Map { entries })
    }

    /// Consumes the current token when it matches `kind`.
    ///
    /// Inputs:
    /// - `kind`: expected token kind.
    ///
    /// Output:
    /// - `true` if a token was consumed.
    ///
    /// Transformation:
    /// - Advances the parser cursor only for exact kind matches.
    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Requires the current token to match `kind`.
    ///
    /// Inputs:
    /// - `kind`: required token kind.
    ///
    /// Output:
    /// - `Some(())` when the token matched and was consumed.
    ///
    /// Transformation:
    /// - Advances the cursor on success and returns `None` on mismatch.
    fn expect(&mut self, kind: TokenKind) -> Option<()> {
        self.consume(kind).then_some(())
    }

    /// Consumes one config identifier token.
    ///
    /// Inputs:
    /// - Cursor positioned at a possible lower identifier.
    ///
    /// Output:
    /// - Identifier text when the current token is a lower identifier.
    ///
    /// Transformation:
    /// - Accepts lexer `Atom` tokens only. Uppercase identifiers are excluded
    ///   from config paths and symbols to match the EBNF.
    fn expect_identifier(&mut self) -> Option<String> {
        let token = self.current()?;
        let kind = token.kind.clone();
        let text = token.text.clone();
        if kind == TokenKind::Atom {
            self.pos += 1;
            Some(text)
        } else {
            None
        }
    }

    /// Checks the current token kind.
    ///
    /// Inputs:
    /// - `kind`: token kind to compare.
    ///
    /// Output:
    /// - `true` when the current token has the requested kind.
    ///
    /// Transformation:
    /// - Reads without advancing the parser cursor.
    fn check(&self, kind: TokenKind) -> bool {
        self.current()
            .map(|token| token.kind == kind)
            .unwrap_or(false)
    }

    /// Returns the current token unless it is EOF.
    ///
    /// Inputs:
    /// - `self`: parser cursor.
    ///
    /// Output:
    /// - Current non-EOF token, or `None` at the end of the token stream.
    ///
    /// Transformation:
    /// - Treats EOF as absence so grammar helpers can use `Option` for
    ///   conservative parse failure.
    fn current(&self) -> Option<&Token> {
        self.tokens
            .get(self.pos)
            .filter(|token| token.kind != TokenKind::EOF)
    }
}
