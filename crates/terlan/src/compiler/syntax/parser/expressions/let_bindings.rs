use super::*;
use crate::terlan_syntax::{COMMA_GROUPED_LET_BINDING_DIAGNOSTIC, REPEATED_LET_BINDING_DIAGNOSTIC};

impl Parser {
    /// Parses a semicolon-scoped local binding expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `let` keyword.
    ///
    /// Output:
    /// - `Expr::Let` containing one ordinary binding or a braced refutable
    ///   group, plus a required final body expression.
    ///
    /// Transformation:
    /// - Collects ordinary repeated `let` bindings into one ordered node so
    ///   large binding sequences do not create a recursion-depth hazard.
    /// - A refutable binding group uses `Pattern <- Expr` entries inside
    ///   braces and shares one `else` block.
    ///   The formatter-only migration mode may consume the retired implicit
    ///   continuation form when no shared fallback is present.
    pub(super) fn parse_let_expr(&mut self) -> ParseResult<Expr> {
        self.expect_keyword(TokenKind::Let)?;
        if self.is_refutable_let_group_start() {
            return self.parse_refutable_let_group();
        }

        let mut bindings = vec![self.parse_let_binding()?];
        let mut implicit_binding_offsets = Vec::new();
        let mut body = None;

        self.reject_comma_grouped_let_binding()?;
        loop {
            if !self.consume_if(TokenKind::Semicolon) {
                break;
            }
            self.skip_let_separator_comments();
            if self.check(TokenKind::Let) {
                if self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| token.kind == TokenKind::LBrace)
                {
                    body = Some(Box::new(self.parse_single_expr()?));
                    break;
                }
                self.expect_keyword(TokenKind::Let)?;
                bindings.push(self.parse_let_binding()?);
                self.reject_comma_grouped_let_binding()?;
            } else if self.is_implicit_let_binding_start() {
                implicit_binding_offsets.push((self.current().start, self.current().span()));
                bindings.push(self.parse_let_binding()?);
                self.reject_comma_grouped_let_binding()?;
            } else {
                body = Some(Box::new(self.parse_expr()?));
                break;
            }
        }

        if let Some((_, span)) = implicit_binding_offsets.first().copied() {
            if self.let_binding_mode == LetBindingMode::MigrateImplicit {
                self.implicit_let_binding_offsets.extend(
                    implicit_binding_offsets
                        .into_iter()
                        .map(|(offset, _)| offset),
                );
            } else {
                return Err(ParseError {
                    message: REPEATED_LET_BINDING_DIAGNOSTIC.to_string(),
                    span,
                });
            }
        }

        if body.is_none() {
            return Err(ParseError {
                message: "let expression requires an explicit result expression".to_string(),
                span: self.current().span(),
            });
        }

        Ok(Expr::Let {
            bindings,
            else_clauses: Vec::new(),
            body,
        })
    }

    fn parse_refutable_let_group(&mut self) -> ParseResult<Expr> {
        self.expect(TokenKind::LBrace)?;
        self.skip_let_separator_comments();
        if self.check(TokenKind::RBrace) {
            return Err(ParseError {
                message: "refutable let group requires at least one binding".to_string(),
                span: self.current().span(),
            });
        }

        let mut bindings = Vec::new();
        loop {
            let pattern = self.parse_pattern()?;
            self.expect(TokenKind::LtMinus)?;
            let value = self.parse_single_expr()?;
            bindings.push(LetBinding { pattern, value });
            self.reject_comma_grouped_let_binding()?;

            if !self.consume_if(TokenKind::Semicolon) {
                break;
            }
            self.skip_let_separator_comments();
            if self.check(TokenKind::RBrace) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.skip_let_separator_comments();
        if !self.consume_keyword("else") {
            return Err(ParseError {
                message: "refutable let group requires an else fallback".to_string(),
                span: self.current().span(),
            });
        }
        self.skip_let_separator_comments();
        self.expect(TokenKind::LBrace)?;
        let else_clauses = self.parse_keyword_expr_clauses(&[TokenKind::RBrace])?;
        self.expect(TokenKind::RBrace)?;
        if else_clauses.is_empty() {
            return Err(ParseError {
                message: "let else requires at least one fallback clause".to_string(),
                span: self.previous().span(),
            });
        }
        self.skip_let_separator_comments();
        self.expect(TokenKind::Semicolon)?;
        let body = Some(Box::new(self.parse_expr()?));
        Ok(Expr::Let {
            bindings,
            else_clauses,
            body,
        })
    }

    fn is_refutable_let_group_start(&self) -> bool {
        if !self.check(TokenKind::LBrace) {
            return false;
        }

        let mut braces = 0usize;
        let mut parens = 0usize;
        let mut brackets = 0usize;
        for (index, token) in self.tokens.iter().enumerate().skip(self.pos) {
            match token.kind {
                TokenKind::LBrace => braces += 1,
                TokenKind::RBrace => {
                    braces = braces.saturating_sub(1);
                    if braces == 0 {
                        return self.tokens[index + 1..]
                            .iter()
                            .find(|token| {
                                !matches!(
                                    token.kind,
                                    TokenKind::Comment
                                        | TokenKind::DocComment
                                        | TokenKind::ModuleDocComment
                                        | TokenKind::DocBlockComment
                                )
                            })
                            .is_some_and(|token| {
                                matches!(token.kind, TokenKind::Atom | TokenKind::Var)
                                    && token.text == "else"
                            });
                    }
                }
                TokenKind::LParen => parens += 1,
                TokenKind::RParen => parens = parens.saturating_sub(1),
                TokenKind::LBracket => brackets += 1,
                TokenKind::RBracket => brackets = brackets.saturating_sub(1),
                TokenKind::LtMinus if braces == 1 && parens == 0 && brackets == 0 => return true,
                TokenKind::EOF => return false,
                _ => {}
            }
        }
        false
    }

    fn skip_let_separator_comments(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Comment
                | TokenKind::DocComment
                | TokenKind::ModuleDocComment
                | TokenKind::DocBlockComment
        ) {
            self.bump();
        }
    }

    fn reject_comma_grouped_let_binding(&self) -> ParseResult<()> {
        if self.current().kind == TokenKind::Comma {
            return Err(ParseError {
                message: COMMA_GROUPED_LET_BINDING_DIAGNOSTIC.to_string(),
                span: self.current().span(),
            });
        }
        Ok(())
    }

    /// Parses one local binding inside a `let` expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a canonical `Pattern`.
    ///
    /// Output:
    /// - A `LetBinding` containing the binding pattern and value expression.
    ///
    /// Transformation:
    /// - Reuses the normal pattern parser so tuple/list/wildcard destructuring
    ///   in let bindings follows the same syntax as case/function patterns.
    fn parse_let_binding(&mut self) -> ParseResult<LetBinding> {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::Equals)?;
        let value = self.parse_single_expr()?;
        Ok(LetBinding { pattern, value })
    }

    /// Reports whether the current cursor starts another `let` binding.
    ///
    /// Inputs:
    /// - Parser cursor after a semicolon inside a `let` expression.
    ///
    /// Output:
    /// - `true` when the next tokens look like `Pattern =`.
    ///
    /// Transformation:
    /// - Performs a non-consuming balanced token scan and checks for a
    ///   top-level `=` so the parser can distinguish another destructuring
    ///   binding from the final body expression.
    fn is_implicit_let_binding_start(&self) -> bool {
        let Some(first) = self.tokens.get(self.pos) else {
            return false;
        };
        if !matches!(
            first.kind,
            TokenKind::Atom
                | TokenKind::Var
                | TokenKind::Int
                | TokenKind::Float
                | TokenKind::String
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::Colon
        ) {
            return false;
        }

        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        let mut index = self.pos;
        let first_is_bare_name = matches!(first.kind, TokenKind::Atom | TokenKind::Var);

        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::LBracket if index == self.pos + 1 && first_is_bare_name => {
                    return false;
                }
                TokenKind::LParen => parens += 1,
                TokenKind::RParen => parens = parens.saturating_sub(1),
                TokenKind::LBracket => brackets += 1,
                TokenKind::RBracket => brackets = brackets.saturating_sub(1),
                TokenKind::LBrace => braces += 1,
                TokenKind::RBrace => braces = braces.saturating_sub(1),
                TokenKind::Equals if parens == 0 && brackets == 0 && braces == 0 => return true,
                TokenKind::Semicolon | TokenKind::Dot | TokenKind::EOF
                    if parens == 0 && brackets == 0 && braces == 0 =>
                {
                    return false;
                }
                _ => {}
            }
            index += 1;
        }

        false
    }
}
