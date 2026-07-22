
impl Parser {
    /// Consumes a fixed lexer keyword token.
    ///
    /// Inputs:
    /// - `expected`: exact token kind expected at the cursor.
    ///
    /// Output:
    /// - `Ok(())` after consuming the token, or a parser diagnostic.
    ///
    /// Transformation:
    /// - Delegates to `expect` and discards the consumed token payload.
    fn expect_keyword(&mut self, expected: TokenKind) -> ParseResult<()> {
        self.expect(expected).map(|_| ())
    }

    /// Consumes an expected contextual keyword.
    ///
    /// Inputs:
    /// - `expected`: lower-case keyword text expected in the current grammar
    ///   position.
    ///
    /// Output:
    /// - `Ok(())` when the current token is the expected contextual keyword.
    ///
    /// Transformation:
    /// - Advances over an identifier token with matching text without making
    ///   the word globally reserved in the lexer.
    fn expect_contextual_keyword(&mut self, expected: &str) -> ParseResult<()> {
        if self.check_keyword(expected) {
            self.pos += 1;
            return Ok(());
        }
        Err(ParseError {
            message: format!("expected `{expected}`"),
            span: self.current().span(),
        })
    }

    /// Consumes a contextual keyword if present.
    ///
    /// Inputs:
    /// - `expected`: lower-case contextual keyword text.
    ///
    /// Output:
    /// - `true` when the token was consumed.
    ///
    /// Transformation:
    /// - Advances over matching identifier-like tokens without reserving the
    ///   word globally.
    fn consume_keyword(&mut self, expected: &str) -> bool {
        if self.check_keyword(expected) {
            self.pos += 1;
            return true;
        }
        false
    }

    /// Parses a source identifier in a permissive identifier position.
    ///
    /// Inputs:
    /// - Parser cursor at an identifier-like token.
    ///
    /// Output:
    /// - Identifier text or a parser diagnostic.
    ///
    /// Transformation:
    /// - Accepts lower and upper identifier tokens and consumes the token.
    fn expect_ident(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            _ => Err(ParseError {
                message: "expected identifier".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses the legacy `:name` spelling into a canonical atom payload.
    fn parse_raw_atom_literal_payload(&mut self) -> ParseResult<String> {
        self.expect(TokenKind::Colon)?;
        let token = self.current().clone();
        let span = token.span();
        let payload = match token.kind {
            TokenKind::Atom => {
                self.bump();
                token.text
            }
            TokenKind::String => {
                self.bump();
                unquote_single_quoted_atom(&token.text).unwrap_or(token.text)
            }
            _ => {
                return Err(ParseError {
                    message: "expected atom literal name after ':'".to_string(),
                    span,
                });
            }
        };
        if payload.is_empty() {
            return Err(ParseError {
                message: "expected non-empty atom literal".to_string(),
                span,
            });
        }
        Ok(payload)
    }

    /// Consumes an exact token kind.
    ///
    /// Inputs:
    /// - `expected`: token kind required at the parser cursor.
    ///
    /// Output:
    /// - Consumed token on success, otherwise a parser diagnostic at the cursor.
    ///
    /// Transformation:
    /// - Advances one token only when the kind matches.
    fn expect(&mut self, expected: TokenKind) -> ParseResult<Token> {
        let token = self.current().clone();
        if token.kind == expected {
            Ok(self.bump())
        } else {
            Err(ParseError {
                message: format!("expected {:?}", expected),
                span: token.span(),
            })
        }
    }

    /// Consumes a token when its kind matches.
    ///
    /// Inputs:
    /// - `kind`: token kind to consume opportunistically.
    ///
    /// Output:
    /// - `true` when a token was consumed.
    ///
    /// Transformation:
    /// - Checks the current token and advances the cursor on match.
    fn consume_if(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Checks the current token kind.
    ///
    /// Inputs:
    /// - `kind`: token kind to compare against the current token.
    ///
    /// Output:
    /// - `true` when the current token kind matches.
    ///
    /// Transformation:
    /// - Reads the cursor without advancing.
    fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    /// Checks whether the current token matches any candidate kind.
    ///
    /// Inputs:
    /// - `kinds`: accepted token kinds.
    ///
    /// Output:
    /// - `true` when any candidate matches the current token.
    ///
    /// Transformation:
    /// - Runs `check` over the candidate list without advancing.
    fn check_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|kind| self.check(kind.clone()))
    }

    /// Skips ordinary comments.
    ///
    /// Inputs:
    /// - Parser cursor at any token.
    ///
    /// Output:
    /// - No return value.
    ///
    /// Transformation:
    /// - Advances over non-doc comments and stops at the first non-comment.
    fn skip_comments(&mut self) {
        while self.check(TokenKind::Comment) {
            self.pos += 1;
        }
    }

    /// Rejects module documentation after the module declaration.
    ///
    /// Inputs:
    /// - Parser cursor at a possible documentation token.
    ///
    /// Output:
    /// - `Ok(())` when no misplaced module docs are present.
    ///
    /// Transformation:
    /// - Converts misplaced `//!` or `@module` block docs into parser errors.
    fn reject_misplaced_module_docs(&self) -> ParseResult<()> {
        if self.check(TokenKind::ModuleDocComment) {
            return Err(ParseError {
                message: "module doc comments (`//!`) must appear before the module declaration"
                    .to_string(),
                span: self.current().span(),
            });
        }
        if self.check(TokenKind::DocBlockComment) && is_module_doc_block(&self.current().text) {
            return Err(ParseError {
                message: "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
                    .to_string(),
                span: self.current().span(),
            });
        }

        Ok(())
    }

    /// Consumes item documentation comments.
    ///
    /// Inputs:
    /// - Parser cursor at zero or more item doc tokens.
    ///
    /// Output:
    /// - Raw documentation token text in source order.
    ///
    /// Transformation:
    /// - Advances over `///` and non-module `/** ... */` doc tokens.
    fn take_item_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::DocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    /// Consumes module documentation comments.
    ///
    /// Inputs:
    /// - Parser cursor at zero or more module doc tokens.
    ///
    /// Output:
    /// - Raw module documentation token text in source order.
    ///
    /// Transformation:
    /// - Advances over `//!` and module doc block tokens.
    fn take_module_docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while self.check(TokenKind::ModuleDocComment) || self.check(TokenKind::DocBlockComment) {
            docs.push(self.bump().text);
        }
        docs
    }

    /// Advances the parser by one token.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - Token that was current before advancing.
    ///
    /// Transformation:
    /// - Clones the token and increments the cursor position.
    fn bump(&mut self) -> Token {
        let token = self.current().clone();
        self.pos += 1;
        token
    }

    /// Returns the previously consumed token.
    ///
    /// Inputs:
    /// - Parser state after at least one token has been consumed.
    ///
    /// Output:
    /// - Reference to the previous token.
    ///
    /// Transformation:
    /// - Indexes the token stream at `pos - 1`.
    fn previous(&self) -> &Token {
        &self.tokens[self.pos - 1]
    }

    /// Checks for a contextual keyword at the cursor.
    ///
    /// Inputs:
    /// - `expected`: contextual keyword text.
    ///
    /// Output:
    /// - `true` when the current identifier-like token has matching text.
    ///
    /// Transformation:
    /// - Treats atom and upper-identifier tokens as contextual keyword carriers.
    fn check_keyword(&self, expected: &str) -> bool {
        matches!(self.current().kind, TokenKind::Atom | TokenKind::Var)
            && self.current().text == expected
    }

    /// Returns the current parser token.
    /// Inputs:
    /// - Current parser cursor.
    /// Output:
    /// - Reference to the current token.
    /// Transformation:
    /// - Indexes the token stream without advancing.
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }
}
