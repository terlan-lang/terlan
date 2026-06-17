use super::*;

impl Parser {
    /// Parses an import declaration after the current token has been identified
    /// as `import`.
    ///
    /// Inputs: the parser cursor must point at the `import` keyword.
    /// Outputs: a module import declaration, asset import declaration, or a
    /// syntax diagnostic.
    /// Transformation: consumes the import token stream and normalizes the
    /// grammar's single `ImportItem` shape into one `ImportDecl` with module
    /// path, imported symbols, type-import flag, or asset source metadata.
    pub(super) fn parse_import(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Import)?;

        let asset_import = if self.consume_keyword("file") {
            Some((ImportKind::File, "file"))
        } else if self.consume_keyword("css") {
            Some((ImportKind::Css, "css"))
        } else if self.consume_keyword("markdown") {
            Some((ImportKind::Markdown, "markdown"))
        } else {
            None
        };

        if let Some((kind, keyword)) = asset_import {
            let raw_path = self.expect(TokenKind::String)?.text.clone();
            let path = raw_path
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(&raw_path)
                .to_string();
            if !self.consume_keyword("as") {
                return Err(ParseError {
                    message: format!("expected `as` in {keyword} import"),
                    span: self.current().span(),
                });
            }
            let alias = self.expect_ident()?;
            let alias_span = Span::new(self.previous().start, self.previous().end);
            self.expect(TokenKind::Dot)?;
            return Ok(Decl::Import(ImportDecl {
                kind,
                module_name: String::new(),
                items: vec![ImportItem {
                    name: alias,
                    as_alias: None,
                    span: alias_span,
                }],
                is_type: false,
                source_path: Some(path),
                span: Span::new(start, self.previous().end),
            }));
        }

        let mut is_type = false;
        if self.consume_if(TokenKind::Type) || self.consume_keyword("type") {
            is_type = true;
        }

        let first_segment = self.expect_ident()?;
        let first_segment_span = self.previous().span();
        let mut path_segments = vec![(first_segment, first_segment_span)];
        let mut brace_import = false;
        while self.check(TokenKind::Dot) {
            if matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if token.kind == TokenKind::LBrace
            ) {
                self.bump();
                brace_import = true;
                break;
            }
            if matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if matches!(token.kind, TokenKind::Atom | TokenKind::Var)
            ) {
                self.bump();
                let segment = self.expect_ident()?;
                let segment_span = self.previous().span();
                path_segments.push((segment, segment_span));
                continue;
            }
            break;
        }

        let mut items = Vec::new();
        let module_name;
        if brace_import && self.consume_if(TokenKind::LBrace) {
            validate_module_path_segments(&path_segments)?;
            module_name = path_segments
                .iter()
                .map(|(segment, _)| segment.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if self.check(TokenKind::RBrace) {
                return Err(ParseError {
                    message: "expected at least one import symbol".to_string(),
                    span: self.current().span(),
                });
            } else {
                loop {
                    let name = self.expect_ident()?;
                    let as_alias = if self.consume_keyword("as") {
                        let alias = self.expect_ident()?;
                        validate_import_alias_class(&name, &alias, self.previous().span())?;
                        Some(alias)
                    } else {
                        None
                    };
                    items.push(ImportItem {
                        name,
                        as_alias,
                        span: Span::new(self.previous().start, self.previous().end),
                    });
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    self.expect(TokenKind::RBrace)?;
                    break;
                }
            }
        } else {
            let (name, _) = path_segments.pop().ok_or_else(|| ParseError {
                message: "expected import item".to_string(),
                span: self.current().span(),
            })?;
            validate_module_path_segments(&path_segments)?;
            module_name = path_segments
                .iter()
                .map(|(segment, _)| segment.as_str())
                .collect::<Vec<_>>()
                .join(".");
            if module_name.is_empty() {
                return Err(ParseError {
                    message: "expected import module".to_string(),
                    span: self.current().span(),
                });
            }
            let as_alias = if self.consume_keyword("as") {
                let alias = self.expect_ident()?;
                validate_import_alias_class(&name, &alias, self.previous().span())?;
                Some(alias)
            } else {
                None
            };
            items.push(ImportItem {
                name,
                as_alias,
                span: Span::new(self.previous().start, self.previous().end),
            });
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Import(ImportDecl {
            kind: ImportKind::Module,
            module_name,
            items,
            is_type,
            source_path: None,
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a canonical Terlan module path.
    ///
    /// Inputs: the parser cursor must point at the first module path segment.
    /// Outputs: the dotted module path string or a syntax diagnostic.
    /// Transformation: consumes a package-rooted module path. The first segment
    /// must be lower-case, while later segments may be lower-case package
    /// segments or upper-case public module namespace segments.
    pub(super) fn parse_module_path(&mut self) -> ParseResult<String> {
        let mut segments = vec![self.expect_package_root_segment()?];
        while self.check(TokenKind::Dot) {
            let dot = self.current().clone();
            let Some(next) = self.tokens.get(self.pos + 1) else {
                break;
            };
            if dot.end != next.start {
                break;
            }
            match next.kind {
                TokenKind::Atom | TokenKind::Var => {
                    self.bump();
                    segments.push(self.expect_module_path_segment()?);
                }
                _ => break,
            }
        }
        Ok(segments.join("."))
    }

    /// Parses the lower-case package root segment of a module path.
    ///
    /// Inputs: the parser cursor must point at the next expected segment.
    /// Outputs: the segment text or a syntax diagnostic at the offending token.
    /// Transformation: consumes only lexer `Atom` tokens for the package root.
    fn expect_package_root_segment(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Var => Err(ParseError {
                message: "expected lower-case package root segment".to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected module path segment".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a non-root module path segment.
    ///
    /// Inputs: the parser cursor must point at a dotted module path segment.
    /// Outputs: the segment text or a syntax diagnostic.
    /// Transformation: consumes lower-case package segments or upper-case
    /// public module namespace segments.
    pub(super) fn expect_module_path_segment(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            _ => Err(ParseError {
                message: "expected module path segment".to_string(),
                span: token.span(),
            }),
        }
    }
}

/// Validates a module path captured by broader import parsing.
///
/// Inputs: ordered module path segments and their source spans.
/// Outputs: `Ok(())` when the package root is lower-case and later segments are
/// identifiers, or a parse diagnostic on an invalid root.
/// Transformation: checks the imported module prefix after the parser has
/// separated it from the imported symbol name, preserving upper-case imported
/// symbols and upper-case module namespace segments.
fn validate_module_path_segments(segments: &[(String, Span)]) -> ParseResult<()> {
    if let Some((segment, span)) = segments.first() {
        if !segment
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            return Err(ParseError {
                message: "expected lower-case package root segment".to_string(),
                span: *span,
            });
        }
    }

    Ok(())
}

/// Validates a normal import alias against the canonical import-symbol class
/// rule.
///
/// Inputs: the imported symbol name, alias text, and alias source span.
/// Outputs: `Ok(())` when both names have the same lower/upper identifier
/// class, or a parse diagnostic anchored to the alias.
/// Transformation: classifies the first character of both names and rejects
/// aliases that would change a lower import into an upper name, or an upper
/// import into a lower name.
fn validate_import_alias_class(name: &str, alias: &str, alias_span: Span) -> ParseResult<()> {
    let name_is_upper = name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase());
    let alias_is_upper = alias
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase());

    if name_is_upper != alias_is_upper {
        return Err(ParseError {
            message: "import alias must preserve identifier class".to_string(),
            span: alias_span,
        });
    }

    Ok(())
}
