use super::super::*;

impl Parser {
    pub(in super::super) fn consume_constraint_list_if_present(
        &mut self,
    ) -> ParseResult<Vec<String>> {
        if !self.consume_if(TokenKind::LBracket) {
            return Ok(Vec::new());
        }
        let mut constraints = Vec::new();
        if self.consume_if(TokenKind::RBracket) {
            return Ok(constraints);
        }
        loop {
            constraints.push(
                self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                    .text,
            );
            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            self.expect(TokenKind::RBracket)?;
            break;
        }
        Ok(constraints)
    }

    pub(in super::super) fn parse_body_expr(&mut self) -> ParseResult<Expr> {
        self.parse_body_expr_with_clause_sep(None, false)
    }

    pub(in super::super) fn parse_body_expr_with_clause_sep(
        &mut self,
        clause_name: Option<&str>,
        is_constructor_clause: bool,
    ) -> ParseResult<Expr> {
        self.skip_comments();
        let mut expr = self.parse_single_expr()?;
        while self.consume_if(TokenKind::Equals) {
            expr = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
        }
        while self.consume_if(TokenKind::Comma) {
            expr = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
        }
        let mut expressions = Vec::new();
        while self.check(TokenKind::Semicolon) {
            if self.is_clause_separator_ahead(clause_name, is_constructor_clause) {
                break;
            }
            self.bump();
            self.skip_comments();
            expressions.push(self.parse_single_expr()?);
        }
        if !expressions.is_empty() {
            let mut values = vec![expr];
            values.append(&mut expressions);
            expr = Expr::Sequence(values);
        }
        Ok(expr)
    }

    fn is_clause_separator_ahead(
        &self,
        clause_name: Option<&str>,
        is_constructor_clause: bool,
    ) -> bool {
        if !matches!(self.current().kind, TokenKind::Semicolon) {
            return false;
        }
        let next = self.tokens.get(self.pos + 1);
        let next_next = self.tokens.get(self.pos + 2);
        if is_constructor_clause {
            return matches!(next, Some(token) if token.kind == TokenKind::LParen);
        }
        let Some(clause_name) = clause_name else {
            return false;
        };
        matches!(next, Some(token) if token.kind == TokenKind::Atom && token.text == clause_name)
            && matches!(next_next, Some(token) if token.kind == TokenKind::LParen)
    }
}
