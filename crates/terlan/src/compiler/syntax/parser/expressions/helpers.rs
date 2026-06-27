use super::*;

impl Parser {
    /// Parses semicolon-separated clauses for keyword expressions.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the first clause pattern.
    /// - `stops`: token kinds that close the surrounding keyword expression.
    ///
    /// Output:
    /// - Ordered `CaseClause` values containing pattern, optional guard, and
    ///   body expression payloads.
    ///
    /// Transformation:
    /// - Parses each clause body as one expression so the following semicolon
    ///   remains a clause separator rather than becoming an expression-sequence
    ///   operator. Nested `let` expressions still own their semicolon-scoped
    ///   binding/body syntax through `parse_single_expr`.
    pub(super) fn parse_keyword_expr_clauses(
        &mut self,
        stops: &[TokenKind],
    ) -> ParseResult<Vec<CaseClause>> {
        let mut clauses = Vec::new();
        if stops.iter().any(|stop| self.check(stop.clone())) {
            return Ok(clauses);
        }
        loop {
            let pattern = self.parse_pattern()?;
            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };
            self.expect(TokenKind::Arrow)?;
            let body = self.parse_single_expr()?;
            clauses.push(CaseClause {
                pattern,
                guard,
                body,
            });
            if self.consume_if(TokenKind::Semicolon) {
                if stops.iter().any(|stop| self.check(stop.clone())) {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(clauses)
    }

    /// Parses a built-in block macro expression.
    ///
    /// Inputs:
    /// - `macro_kind`: built-in block macro selected by the caller.
    /// - `raw`: preserved raw block text.
    ///
    /// Output:
    /// - Parsed macro expression with structured payload when available.
    ///
    /// Transformation:
    /// - Converts the raw macro block into the parser expression shape while
    ///   preserving original text for syntax output and diagnostics.
    pub(super) fn parse_builtin_block_macro(
        &mut self,
        macro_kind: BuiltinBlockMacro,
        raw: String,
    ) -> ParseResult<Expr> {
        match macro_kind {
            BuiltinBlockMacro::Html => {
                let nodes = parse_html_nodes(&raw);
                Ok(Expr::HtmlBlock(HtmlBlockExpr {
                    macro_kind,
                    raw,
                    nodes,
                }))
            }
        }
    }

    /// Parses one list-comprehension generator.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the generator pattern.
    ///
    /// Output:
    /// - Generator pattern and source expression.
    ///
    /// Transformation:
    /// - Consumes `Pattern <- Expr` for the formal list comprehension surface.
    pub(super) fn parse_list_generator(&mut self) -> ParseResult<(Pattern, Expr)> {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::LtMinus)?;
        let source = self.parse_expr()?;
        Ok((pattern, source))
    }

    /// Parses a comma-separated expression list.
    ///
    /// Inputs:
    /// - `end`: token kind that terminates the list.
    /// - Parser cursor positioned at the first expression or terminator.
    ///
    /// Output:
    /// - Ordered expression arguments/elements.
    ///
    /// Transformation:
    /// - Consumes zero or more expressions separated by commas without
    ///   consuming the closing terminator token.
    pub(super) fn parse_expr_list(&mut self, end: TokenKind) -> ParseResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.check(end) {
            return Ok(args);
        }

        loop {
            args.push(self.parse_expr()?);
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }

        Ok(args)
    }

    /// Parses a comma-separated call-argument list.
    ///
    /// Inputs:
    /// - `end`: token kind that terminates the call argument list.
    /// - Parser cursor positioned at the first argument or terminator.
    ///
    /// Output:
    /// - Ordered argument expressions and a parallel optional-name vector.
    ///
    /// Transformation:
    /// - Parses `name = expr` as named call-site arguments, enforces that
    ///   positional arguments cannot follow named arguments, and leaves the
    ///   closing terminator for the caller to consume.
    pub(super) fn parse_call_arg_list(
        &mut self,
        end: TokenKind,
    ) -> ParseResult<(Vec<Expr>, Vec<Option<String>>)> {
        let mut args = Vec::new();
        let mut arg_names = Vec::new();
        let mut seen_named = false;
        if self.check(end) {
            return Ok((args, arg_names));
        }

        loop {
            let named = self.peek_named_call_arg_name();
            if let Some(name) = named {
                self.bump();
                self.expect(TokenKind::Equals)?;
                args.push(self.parse_expr()?);
                arg_names.push(Some(name));
                seen_named = true;
            } else {
                if seen_named {
                    return Err(ParseError {
                        message: "positional arguments must come before named arguments"
                            .to_string(),
                        span: self.current().span(),
                    });
                }
                args.push(self.parse_expr()?);
                arg_names.push(None);
            }

            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }

        Ok((args, arg_names))
    }

    /// Returns a named call argument key at the current cursor.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a possible argument name.
    ///
    /// Output:
    /// - `Some(name)` when the current token is a source identifier immediately
    ///   followed by `=`.
    /// - `None` otherwise.
    ///
    /// Transformation:
    /// - Performs two-token lookahead only; it does not consume input.
    fn peek_named_call_arg_name(&self) -> Option<String> {
        let current = self.tokens.get(self.pos)?;
        let next = self.tokens.get(self.pos + 1)?;
        if matches!(current.kind, TokenKind::Atom | TokenKind::Var)
            && next.kind == TokenKind::Equals
        {
            Some(current.text.clone())
        } else {
            None
        }
    }

    /// Parses one `if` clause condition.
    ///
    /// Inputs:
    /// - Parser cursor positioned at an `if` clause condition.
    ///
    /// Output:
    /// - Parsed condition expression.
    ///
    /// Transformation:
    /// - Accepts `_` only in this clause-head position and normalizes it to
    ///   the existing `true` expression so downstream CoreIR and emitters keep
    ///   one boolean fallback representation.
    pub(super) fn parse_if_condition(&mut self) -> ParseResult<Expr> {
        let token = self.current().clone();
        if token.kind == TokenKind::Atom && token.text == "_" {
            self.bump();
            Ok(Expr::Var("true".to_string()))
        } else {
            self.parse_single_expr()
        }
    }

    /// Parses a canonical `Atom["name"]` value expression.
    ///
    /// Inputs:
    /// - Parser cursor at the leading `Atom` name token.
    ///
    /// Output:
    /// - `Expr::AtomLiteral` containing the unescaped symbolic payload.
    ///
    /// Transformation:
    /// - Consumes the `Atom[StringLiteral]` token sequence, validates that the
    ///   payload is non-empty, and converts supported string escapes into the
    ///   runtime atom text carried by the syntax tree.
    pub(super) fn parse_atom_literal_expr(&mut self) -> ParseResult<Expr> {
        let start = self.expect(TokenKind::Var)?.span();
        self.expect(TokenKind::LBracket)?;
        let literal = self.expect(TokenKind::String)?;
        self.expect(TokenKind::RBracket)?;
        let payload = parse_atom_string_literal_token(&literal).ok_or_else(|| ParseError {
            message: "expected non-empty atom string literal".to_string(),
            span: start,
        })?;
        Ok(Expr::AtomLiteral(payload))
    }
}
