use super::*;

impl Parser {
    /// Parses a semicolon-scoped local binding expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `let` keyword.
    ///
    /// Output:
    /// - `Expr::Let` with one or more ordered bindings and a required final
    ///   body expression.
    ///
    /// Transformation:
    /// - Consumes `let Binding = Expr` pairs separated by semicolons. A
    ///   semicolon followed by another `Binding =` starts the next binding;
    ///   otherwise the semicolon starts the required final body expression.
    pub(super) fn parse_let_expr(&mut self) -> ParseResult<Expr> {
        self.expect_keyword(TokenKind::Let)?;
        let mut bindings = vec![self.parse_let_binding()?];
        let mut body = None;

        while self.consume_if(TokenKind::Semicolon) {
            if self.is_let_binding_start() {
                bindings.push(self.parse_let_binding()?);
            } else {
                body = Some(Box::new(self.parse_expr()?));
                break;
            }
        }

        if body.is_none() {
            return Err(ParseError {
                message: "let expression requires an explicit result expression".to_string(),
                span: self.current().span(),
            });
        }

        Ok(Expr::Let { bindings, body })
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
    fn is_let_binding_start(&self) -> bool {
        let Some(first) = self.tokens.get(self.pos) else {
            return false;
        };
        if !matches!(
            first.kind,
            TokenKind::Atom
                | TokenKind::Var
                | TokenKind::Int
                | TokenKind::Float
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
