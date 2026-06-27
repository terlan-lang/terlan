use super::super::*;
use super::ExprFieldKind;

impl Parser {
    /// Parses postfix expression suffixes after a primary expression.
    ///
    /// Inputs:
    /// - `expr`: already parsed expression that can receive postfix suffixes.
    ///
    /// Output:
    /// - Expression with all immediately following postfix suffixes applied.
    ///
    /// Transformation:
    /// - Repeatedly consumes record access/update, constructor extension,
    ///   template instantiation, method call, field access, function-value
    ///   invocation, and index suffixes, folding each suffix into the current
    ///   expression tree.
    pub(super) fn parse_expr_suffix(&mut self, mut expr: Expr) -> ParseResult<Expr> {
        loop {
            if self.consume_if(TokenKind::Hash) {
                let name = self.expect_ident()?;
                if self.consume_if(TokenKind::LBrace) {
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                    }

                    expr = Expr::RecordUpdate {
                        value: Box::new(expr),
                        name,
                        fields,
                    };
                    continue;
                }

                self.expect(TokenKind::Dot)?;
                let field = self.expect_ident()?;
                expr = Expr::RecordAccess {
                    value: Box::new(expr),
                    name,
                    field,
                };
                continue;
            }

            if self.consume_if(TokenKind::With) {
                if !matches!(expr, Expr::Call { .. }) {
                    return Err(ParseError {
                        message: "constructor chain requires call expression before with"
                            .to_string(),
                        span: self.current().span(),
                    });
                }
                let record = self.parse_record_expr()?;
                expr = Expr::ConstructorChain {
                    base: Box::new(expr),
                    record: Box::new(record),
                };
                continue;
            }

            if self.check(TokenKind::LBrace)
                && matches!(expr, Expr::Var(_))
                && matches!(
                    self.tokens.get(self.pos.saturating_sub(1)),
                    Some(previous) if previous.end == self.current().start
                )
            {
                self.bump();
                let Expr::Var(name) = expr else {
                    unreachable!("template instantiation receiver was checked above");
                };
                let mut fields = Vec::new();
                if !self.consume_if(TokenKind::RBrace) {
                    loop {
                        fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                expr = Expr::TemplateInstantiate { name, fields };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                    (self.tokens.get(self.pos), self.tokens.get(self.pos + 1)),
                    (Some(dot), Some(token))
                        if dot.end == token.start && token.kind == TokenKind::LParen
                )
            {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    type_args: Vec::new(),
                    args,
                    arg_names,
                    remote: None,
                    is_fun_value: true,
                };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                    (
                        self.tokens.get(self.pos),
                        self.tokens.get(self.pos + 1),
                        self.tokens.get(self.pos + 2)
                    ),
                    (Some(dot), Some(name), Some(open))
                        if dot.end == name.start
                            && name.end == open.start
                            && matches!(name.kind, TokenKind::Atom | TokenKind::Var)
                            && (open.kind == TokenKind::LParen || open.kind == TokenKind::LBracket)
                )
            {
                self.bump();
                let field = self.expect_lower_ident("expected lower-case method name")?;
                let type_args = self.parse_optional_call_type_args()?;
                self.expect(TokenKind::LParen)?;
                let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(Expr::FieldAccess {
                        value: Box::new(expr),
                        field,
                    }),
                    type_args,
                    args,
                    arg_names,
                    remote: None,
                    is_fun_value: false,
                };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                    (
                        self.tokens.get(self.pos),
                        self.tokens.get(self.pos + 1),
                        self.tokens.get(self.pos + 2)
                    ),
                    (Some(dot), Some(hash), Some(name))
                        if dot.end == hash.start
                            && hash.end == name.start
                            && matches!(name.kind, TokenKind::Atom | TokenKind::Var)
                )
            {
                self.bump();
                self.expect(TokenKind::Hash)?;
                let field = self.expect_lower_ident("expected lower-case private field name")?;
                expr = Expr::FieldAccess {
                    value: Box::new(expr),
                    field: format!("#{field}"),
                };
                continue;
            }

            if self.check(TokenKind::Dot)
                && matches!(
                    (self.tokens.get(self.pos), self.tokens.get(self.pos + 1)),
                    (Some(dot), Some(token))
                        if dot.end == token.start
                            && matches!(token.kind, TokenKind::Atom | TokenKind::Var)
                )
            {
                self.bump();
                let field = self.expect_lower_ident("expected lower-case field name")?;
                expr = Expr::FieldAccess {
                    value: Box::new(expr),
                    field,
                };
                continue;
            }

            if self.consume_if(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index(Box::new(expr), Box::new(index));
                continue;
            }

            break;
        }

        Ok(expr)
    }

    /// Parses Terlan record construction.
    ///
    /// Inputs:
    /// - Parser cursor at the record type name.
    ///
    /// Output:
    /// - Record construction expression with parsed field assignments.
    ///
    /// Transformation:
    /// - Consumes `TypeName { field = expr }` syntax and reuses expression
    ///   field parsing so record construction follows the same field rules as
    ///   template instantiation.
    fn parse_record_expr(&mut self) -> ParseResult<Expr> {
        let name = self.expect(TokenKind::Var)?.text;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;
        }
        Ok(Expr::RecordConstruct { name, fields })
    }

    /// Parses one expression field in a map, Terlan record/template, or
    /// Erlang record interop expression.
    ///
    /// Inputs: the parser cursor must point at the field key and `kind`
    /// selects the grammar context for accepted key classes and separators.
    /// Output: a structured expression field, or a syntax diagnostic at the
    /// offending token.
    /// Transformation: consumes the key, field separator, and value expression,
    /// preserving the key spelling in the syntax tree.
    pub(super) fn parse_record_expr_field(
        &mut self,
        kind: ExprFieldKind,
    ) -> ParseResult<MapExprField> {
        let parsed_key = if kind == ExprFieldKind::TerlanRecord {
            Some(self.parse_record_field_key("expected lower-case field name")?)
        } else {
            None
        };
        let key_token = self.current().clone();
        let valid_key = match kind {
            ExprFieldKind::Map => {
                key_token.kind == TokenKind::Atom || key_token.kind == TokenKind::Var
            }
            ExprFieldKind::TerlanRecord => true,
        };
        if !valid_key {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::TerlanRecord => "expected lower-case field name".to_string(),
                    ExprFieldKind::Map => "expected map field key atom".to_string(),
                },
                span: key_token.span(),
            });
        }

        if parsed_key.is_none() {
            self.bump();
        }
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) && kind == ExprFieldKind::Map {
            false
        } else {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::Map => "expected := or => in map expression".to_string(),
                    ExprFieldKind::TerlanRecord => "expected = in struct field".to_string(),
                },
                span: self.current().span(),
            });
        };

        let value = self.parse_expr()?;
        let key = parsed_key
            .as_ref()
            .map(Self::field_key_text)
            .unwrap_or_else(|| key_token.text);
        Ok(MapExprField {
            key,
            value: Box::new(value),
            required,
        })
    }
}
