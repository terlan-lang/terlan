use super::*;

/// Expression field grammar context for key class and separator validation.
///
/// Inputs: selected by the caller based on the production being parsed.
/// Output: passed to expression-field parsing as a compact policy value.
/// Transformation: distinguishes Terlan source records/templates from Erlang
/// record interop without changing the emitted field representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExprFieldKind {
    Map,
    TerlanRecord,
    ErlangRecord,
}

impl Parser {
    /// Parses a full expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the first expression token.
    ///
    /// Output:
    /// - A single expression, or `Expr::Sequence` for top-level semicolon
    ///   separated expression forms.
    ///
    /// Transformation:
    /// - Parses the first expression through the formal precedence chain, then
    ///   folds following semicolon-separated expressions into a sequence.
    pub(super) fn parse_expr(&mut self) -> ParseResult<Expr> {
        let first = self.parse_single_expr()?;
        if !self.check(TokenKind::Semicolon) {
            return Ok(first);
        }

        let mut expressions = vec![first];
        while self.consume_if(TokenKind::Semicolon) {
            expressions.push(self.parse_single_expr()?);
        }
        Ok(Expr::Sequence(expressions))
    }
    /// Parses one non-sequence expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a single expression form.
    ///
    /// Output:
    /// - Parsed expression without consuming outer sequence separators.
    ///
    /// Transformation:
    /// - Routes `let` expressions before falling through to assignment and
    ///   precedence parsing.
    pub(super) fn parse_single_expr(&mut self) -> ParseResult<Expr> {
        if self.check(TokenKind::Let) {
            return self.parse_let_expr();
        }
        self.parse_assignment_expr()
    }
    /// Parses assignment-like expression forms.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the start of a non-`let` expression.
    ///
    /// Output:
    /// - A normal expression when no assignment operator is present.
    /// - `Expr::IndexAssign` for `collection[index] = value`.
    ///
    /// Transformation:
    /// - Parses the left side using the ordinary precedence parser, then
    ///   accepts `=` only when the left side is an indexed expression. This
    ///   deliberately does not introduce general variable assignment.
    fn parse_assignment_expr(&mut self) -> ParseResult<Expr> {
        let left = self.parse_binary_expr(0)?;
        if !self.consume_if(TokenKind::Equals) {
            return Ok(left);
        }

        let Expr::Index(collection, index) = left else {
            return Err(ParseError {
                message: "assignment is only supported for indexed collection updates".to_string(),
                span: self.previous().span(),
            });
        };
        let value = self.parse_single_expr()?;
        Ok(Expr::IndexAssign {
            collection,
            index,
            value: Box::new(value),
        })
    }
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
    fn parse_let_expr(&mut self) -> ParseResult<Expr> {
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
    /// - Parser cursor positioned at a canonical `Binding` name.
    ///
    /// Output:
    /// - A `LetBinding` containing the binding name and value expression.
    ///
    /// Transformation:
    /// - Accepts only lower-case binding names or ignored lower-case binding
    ///   names and rejects wildcard-only or uppercase constructor syntax.
    fn parse_let_binding(&mut self) -> ParseResult<LetBinding> {
        let name = self.expect_binding_name()?;
        self.expect(TokenKind::Equals)?;
        let value = self.parse_single_expr()?;
        Ok(LetBinding { name, value })
    }
    /// Reports whether the current cursor starts another `let` binding.
    ///
    /// Inputs:
    /// - Parser cursor after a semicolon inside a `let` expression.
    ///
    /// Output:
    /// - `true` when the next token pair is `Binding =`.
    ///
    /// Transformation:
    /// - Performs a non-consuming two-token lookahead so the parser can
    ///   distinguish another binding from the final body expression.
    fn is_let_binding_start(&self) -> bool {
        let Some(next) = self.tokens.get(self.pos + 1) else {
            return false;
        };
        self.is_binding_token(self.current()) && next.kind == TokenKind::Equals
    }
    /// Parses binary, cast, boolean, and pipe expressions by precedence.
    ///
    /// Inputs:
    /// - `min_prec`: lowest precedence accepted by the current recursive step.
    /// - Parser cursor positioned at the left operand.
    ///
    /// Output:
    /// - Expression tree preserving the formal operator precedence model.
    ///
    /// Transformation:
    /// - Applies precedence climbing over unary operands and rejects deprecated
    ///   Erlang-style equality/inequality operators at parse time.
    fn parse_binary_expr(&mut self, min_prec: u8) -> ParseResult<Expr> {
        let mut left = self.parse_unary_expr()?;

        loop {
            if self.check_keyword("as") {
                let prec = 8;
                if prec < min_prec {
                    break;
                }
                self.consume_keyword("as");
                let target_type = self.parse_cast_target_type()?;
                left = Expr::Cast {
                    expr: Box::new(left),
                    target_type,
                };
                continue;
            }

            let (op, prec) = match self.current().kind {
                TokenKind::Plus => (Some(BinaryOp::Add), 6),
                TokenKind::Minus => (Some(BinaryOp::Sub), 6),
                TokenKind::Star => (Some(BinaryOp::Mul), 7),
                TokenKind::Slash => (Some(BinaryOp::Div), 7),
                TokenKind::EqEq => (Some(BinaryOp::EqEq), 5),
                TokenKind::EqEqEq => {
                    return Err(ParseError {
                        message: "deprecated equality operator '=:=', use '=='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::NotEq if self.current().text == "!=" => (Some(BinaryOp::NotEq), 5),
                TokenKind::NotEq => {
                    return Err(ParseError {
                        message: "deprecated inequality operator '/=', use '!='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::NotEqEq => {
                    return Err(ParseError {
                        message: "deprecated inequality operator '=/=', use '!='".to_string(),
                        span: self.current().span(),
                    });
                }
                TokenKind::Lt => (Some(BinaryOp::Lt), 5),
                TokenKind::Gt => (Some(BinaryOp::Gt), 5),
                TokenKind::LtEq => (Some(BinaryOp::LtEq), 5),
                TokenKind::GtEq => (Some(BinaryOp::GtEq), 5),
                TokenKind::DivRem => (Some(BinaryOp::DivRem), 7),
                TokenKind::Rem => (Some(BinaryOp::Rem), 7),
                TokenKind::And => (Some(BinaryOp::And), 4),
                TokenKind::Or => (Some(BinaryOp::Or), 3),
                TokenKind::PipeForward => (Some(BinaryOp::PipeForward), 2),
                _ => (None, 0),
            };

            let (op, prec) = match op {
                Some(op) => (op, prec),
                None => break,
            };

            if prec < min_prec {
                break;
            }

            self.bump();
            let right = self.parse_binary_expr(prec + 1)?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }
    /// Parses the target type for an explicit cast expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned immediately after contextual keyword `as`.
    ///
    /// Output:
    /// - A preserved `TypeExpr` naming the requested conversion target.
    ///
    /// Transformation:
    /// - Consumes type syntax until the next expression boundary. This keeps
    ///   `value as Type` below unary/postfix syntax and above arithmetic,
    ///   comparison, boolean, and pipe operators.
    fn parse_cast_target_type(&mut self) -> ParseResult<TypeExpr> {
        self.parse_type_expr(&[
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::EqEq,
            TokenKind::EqEqEq,
            TokenKind::NotEq,
            TokenKind::NotEqEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::DivRem,
            TokenKind::Rem,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::PipeForward,
            TokenKind::Bang,
            TokenKind::Comma,
            TokenKind::Semicolon,
            TokenKind::RParen,
            TokenKind::RBracket,
            TokenKind::RBrace,
            TokenKind::End,
            TokenKind::After,
            TokenKind::Catch,
            TokenKind::When,
            TokenKind::Arrow,
            TokenKind::Dot,
            TokenKind::EOF,
        ])
    }
    /// Parses one expression operand from the token stream.
    ///
    /// Input: the parser cursor at the start of an expression operand, including
    /// literals, names, keyword expressions, macro forms, and prefix operators.
    ///
    /// Output: an expression tree whose suffixes are added by the caller after
    /// the primary operand is recognized.
    ///
    /// Transformation: source tokens are classified according to the formal
    /// expression grammar; pattern-only wildcard syntax is rejected here so `_`
    /// cannot leak into value position.
    fn parse_unary_expr(&mut self) -> ParseResult<Expr> {
        let token = self.current().clone();
        let token_kind = token.kind.clone();
        let expr = match token_kind {
            TokenKind::Int => {
                self.bump();
                Expr::Int(parse_int_literal_token(&token)?)
            }
            TokenKind::Float => {
                self.bump();
                Expr::Float(token.text.parse::<f64>().unwrap_or(0.0))
            }
            TokenKind::Minus => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Bang => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Bang,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Atom if token.text == "not" => {
                self.bump();
                Expr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary_expr()?),
                }
            }
            TokenKind::Atom if token.text == "quote" => {
                self.bump();
                let inner = self.parse_single_expr()?;
                Expr::Quote(Box::new(inner))
            }
            TokenKind::Atom if token.text == "unquote" => {
                self.bump();
                self.expect(TokenKind::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Expr::Unquote(Box::new(inner))
            }
            TokenKind::Colon => {
                self.bump();
                Expr::Atom(self.expect_atom_literal_name()?)
            }
            TokenKind::Question => {
                self.bump();
                let name = self.expect_ident()?;
                let args = if self.consume_if(TokenKind::LParen) {
                    let args = self.parse_expr_list(TokenKind::RParen)?;
                    self.expect(TokenKind::RParen)?;
                    args
                } else {
                    Vec::new()
                };
                Expr::MacroCall { name, args }
            }
            TokenKind::Atom
                if BuiltinBlockMacro::from_name(&token.text).is_some()
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(token) if token.kind == TokenKind::LBrace
                    ) =>
            {
                let macro_kind =
                    BuiltinBlockMacro::from_name(&token.text).expect("known block macro");
                self.bump();
                let raw = self.parse_raw_block()?;
                self.parse_builtin_block_macro(macro_kind, raw)?
            }
            TokenKind::Atom
                if token.text != "_"
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(next) if next.kind == TokenKind::LBrace && token.end == next.start
                    ) =>
            {
                self.bump();
                let raw = self.parse_raw_block()?;
                Expr::RawMacro {
                    name: token.text.clone(),
                    raw,
                }
            }
            TokenKind::Atom | TokenKind::Var => {
                if token.text == "Atom"
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(next) if next.kind == TokenKind::LBracket
                    )
                {
                    return self.parse_atom_literal_expr();
                }

                self.bump();
                let base_expr = if token_kind == TokenKind::Atom {
                    if token.text == "_" {
                        return Err(ParseError {
                            message: "wildcard '_' is only valid in pattern position".to_string(),
                            span: token.span(),
                        });
                    } else {
                        Expr::Var(token.text.clone())
                    }
                } else {
                    Expr::Var(token.text.clone())
                };

                if self.consume_if(TokenKind::Colon) {
                    let fun =
                        self.expect_lower_ident("expected lower-case remote function name")?;
                    self.expect(TokenKind::LParen)?;
                    let args = self.parse_expr_list(TokenKind::RParen)?;
                    self.expect(TokenKind::RParen)?;
                    Expr::Call {
                        callee: Box::new(Expr::Atom(fun)),
                        args,
                        remote: Some(token.text.clone()),
                        is_fun_value: false,
                    }
                } else {
                    let mut dotted = vec![token.text.clone()];
                    let mut lookahead = self.pos;
                    if token_kind == TokenKind::Var
                        && matches!(
                            self.tokens.get(lookahead),
                            Some(token) if token.kind == TokenKind::LBracket
                        )
                    {
                        let type_args = self.parse_required_trait_call_type_arg_text()?;
                        self.expect(TokenKind::Dot)?;
                        let fun =
                            self.expect_lower_ident("expected lower-case trait method name")?;
                        self.expect(TokenKind::LParen)?;
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            args,
                            remote: Some(format!("{}{type_args}", token.text)),
                            is_fun_value: false,
                        });
                    }

                    while matches!(self.tokens.get(lookahead), Some(token) if token.kind == TokenKind::Dot)
                        && matches!(
                            (self.tokens.get(lookahead), self.tokens.get(lookahead + 1)),
                            (Some(dot), Some(token))
                                if dot.end == token.start
                                    && matches!(token.kind, TokenKind::Atom | TokenKind::Var)
                        )
                    {
                        dotted.push(self.tokens[lookahead + 1].text.clone());
                        lookahead += 2;
                    }

                    if dotted.len() > 1
                        && matches!(
                            self.tokens.get(lookahead),
                            Some(token) if token.kind == TokenKind::LParen
                        )
                    {
                        if token_kind == TokenKind::Atom
                            && dotted.len() == 2
                            && matches!(
                                self.tokens.get(self.pos),
                                Some(dot) if dot.kind == TokenKind::Dot
                            )
                            && matches!(
                                self.tokens.get(self.pos + 1),
                                Some(name) if name.kind == TokenKind::Atom
                            )
                        {
                            self.expect(TokenKind::Dot)?;
                            let field =
                                self.expect_lower_ident("expected lower-case method name")?;
                            self.expect(TokenKind::LParen)?;
                            let args = self.parse_expr_list(TokenKind::RParen)?;
                            self.expect(TokenKind::RParen)?;
                            return Ok(Expr::Call {
                                callee: Box::new(Expr::FieldAccess {
                                    value: Box::new(base_expr),
                                    field,
                                }),
                                args,
                                remote: None,
                                is_fun_value: false,
                            });
                        }

                        if token_kind != TokenKind::Atom {
                            if dotted.len() == 2 {
                                self.expect(TokenKind::Dot)?;
                                let fun = self.expect_lower_ident(
                                    "expected lower-case remote function name",
                                )?;
                                self.expect(TokenKind::LParen)?;
                                let args = self.parse_expr_list(TokenKind::RParen)?;
                                self.expect(TokenKind::RParen)?;
                                return Ok(Expr::Call {
                                    callee: Box::new(Expr::Atom(fun)),
                                    args,
                                    remote: Some(token.text.clone()),
                                    is_fun_value: false,
                                });
                            }

                            return Err(ParseError {
                                message: "expected lower-case package root segment".to_string(),
                                span: token.span(),
                            });
                        }

                        let mut remote_parts = vec![token.text.clone()];
                        let mut fun = String::new();
                        for index in 1..dotted.len() {
                            self.expect(TokenKind::Dot)?;
                            if index == dotted.len() - 1 {
                                fun = self.expect_lower_ident(
                                    "expected lower-case remote function name",
                                )?;
                            } else {
                                remote_parts.push(self.expect_module_path_segment()?);
                            }
                        }
                        self.expect(TokenKind::LParen)?;
                        let remote = remote_parts.join(".");
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            args,
                            remote: Some(remote),
                            is_fun_value: false,
                        }
                    } else if self.consume_if(TokenKind::LParen) {
                        let args = self.parse_expr_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(base_expr),
                            args,
                            remote: None,
                            is_fun_value: false,
                        }
                    } else {
                        base_expr
                    }
                }
            }
            TokenKind::String => {
                self.bump();
                Expr::Binary(token.text)
            }
            TokenKind::Binary => {
                self.bump();
                Expr::Binary(token.text)
            }
            TokenKind::LParen => self.parse_paren_or_lambda_expr()?,
            TokenKind::LBracket => {
                self.bump();
                if self.check(TokenKind::RBracket) {
                    self.bump();
                    Expr::List(Vec::new())
                } else {
                    let first = self.parse_expr()?;
                    if self.consume_if(TokenKind::Pipe) {
                        let checkpoint = self.pos;
                        let generator = self.parse_list_generator();
                        match generator {
                            Ok((pattern, source)) => {
                                let mut guard = None;
                                while self.consume_if(TokenKind::Comma) {
                                    let qualifier_checkpoint = self.pos;
                                    if self.parse_list_generator().is_ok() {
                                        return Err(ParseError {
                                            message: "multiple list comprehension generators are not supported in the formal parser path".to_string(),
                                            span: self.current().span(),
                                        });
                                    }
                                    self.pos = qualifier_checkpoint;
                                    let filter = self.parse_expr()?;
                                    guard = Some(combine_comprehension_filter_guard(guard, filter));
                                }
                                self.expect(TokenKind::RBracket)?;
                                Expr::ListComprehension {
                                    expr: Box::new(first),
                                    pattern,
                                    source: Box::new(source),
                                    guard,
                                }
                            }
                            Err(_) => {
                                self.pos = checkpoint;
                                let tail = self.parse_expr()?;
                                self.expect(TokenKind::RBracket)?;
                                Expr::ListCons(Box::new(first), Box::new(tail))
                            }
                        }
                    } else {
                        let mut items = vec![first];
                        while self.consume_if(TokenKind::Comma) {
                            items.push(self.parse_expr()?);
                        }
                        self.expect(TokenKind::RBracket)?;
                        Expr::List(items)
                    }
                }
            }
            TokenKind::LBrace => {
                self.bump();
                if self.check(TokenKind::RBrace) {
                    self.bump();
                    Expr::Tuple(Vec::new())
                } else {
                    let mut items = Vec::new();
                    loop {
                        items.push(self.parse_expr()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace)?;
                    Expr::Tuple(items)
                }
            }
            TokenKind::Hash => {
                self.bump();
                if self.consume_if(TokenKind::LBracket) {
                    if self.check(TokenKind::RBracket) {
                        self.bump();
                        Expr::FixedArray(Vec::new())
                    } else {
                        let mut elements = Vec::new();
                        loop {
                            elements.push(self.parse_expr()?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBracket)?;
                        Expr::FixedArray(elements)
                    }
                } else if self.consume_if(TokenKind::LBrace) {
                    if self.consume_if(TokenKind::RBrace) {
                        Expr::Map(Vec::new())
                    } else {
                        let mut fields = Vec::new();
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::Map)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                        Expr::Map(fields)
                    }
                } else {
                    let name = self.expect_ident()?;
                    self.expect(TokenKind::LBrace)?;
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::ErlangRecord)?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RBrace)?;
                    }
                    Expr::RecordConstruct { name, fields }
                }
            }
            TokenKind::Case => {
                self.bump();
                let scrutinee = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let clauses = self.parse_keyword_expr_clauses(&[TokenKind::RBrace])?;
                self.expect(TokenKind::RBrace)?;
                Expr::Case {
                    scrutinee: Box::new(scrutinee),
                    clauses,
                }
            }
            TokenKind::Try => {
                self.bump();
                let body = self.parse_expr()?;
                self.expect(TokenKind::LBrace)?;
                let of_clauses = self.parse_keyword_expr_clauses(&[
                    TokenKind::Catch,
                    TokenKind::After,
                    TokenKind::RBrace,
                ])?;
                let mut catch_clauses = Vec::new();
                if self.consume_if(TokenKind::Catch) {
                    catch_clauses =
                        self.parse_keyword_expr_clauses(&[TokenKind::After, TokenKind::RBrace])?;
                }
                let after_clause = if self.consume_if(TokenKind::After) {
                    let trigger = self.parse_expr()?;
                    self.expect(TokenKind::Arrow)?;
                    let body = self.parse_expr()?;
                    Some(TryAfterClause {
                        trigger: Box::new(trigger),
                        body: Box::new(body),
                    })
                } else {
                    None
                };
                self.expect(TokenKind::RBrace)?;
                Expr::Try {
                    body: Box::new(body),
                    of_clauses,
                    catch_clauses,
                    after_clause,
                }
            }
            TokenKind::If => {
                self.bump();
                self.expect(TokenKind::LBrace)?;
                let mut clauses = Vec::new();
                loop {
                    let condition = self.parse_single_expr()?;
                    self.expect(TokenKind::Arrow)?;
                    let body = self.parse_single_expr()?;
                    clauses.push(IfClause { condition, body });
                    if self.consume_if(TokenKind::Semicolon) {
                        if self.check(TokenKind::RBrace) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                self.expect(TokenKind::RBrace)?;
                Expr::If { clauses }
            }
            TokenKind::Comment
            | TokenKind::DocComment
            | TokenKind::ModuleDocComment
            | TokenKind::DocBlockComment => {
                self.bump();
                self.parse_unary_expr()?
            }
            other => {
                return Err(ParseError {
                    message: format!("unexpected token {:?} in expression", other),
                    span: token.span(),
                })
            }
        };

        self.parse_expr_suffix(expr)
    }
    /// Parses explicit type arguments in a trait-targeted method call.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `[` in `Trait[Type].method(...)`.
    ///
    /// Output:
    /// - Bracketed type-argument text such as `[Int]` or `[List[String], T]`.
    ///
    /// Transformation:
    /// - Reuses the formal type-expression parser for each comma-separated
    ///   argument and preserves canonical spacing so syntax output and backend
    ///   trait lookup use the same normalized qualifier text.
    fn parse_required_trait_call_type_arg_text(&mut self) -> ParseResult<String> {
        if !self.consume_if(TokenKind::LBracket) {
            return Err(ParseError {
                message: "expected trait call type arguments".to_string(),
                span: self.current().span(),
            });
        }

        let mut args = Vec::new();
        if self.check(TokenKind::RBracket) {
            return Err(ParseError {
                message: "trait call type arguments cannot be empty".to_string(),
                span: self.current().span(),
            });
        }

        loop {
            args.push(
                self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?
                    .text,
            );
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBracket)?;

        Ok(format!("[{}]", args.join(", ")))
    }
    /// Parses either a parenthesized expression or the canonical lambda syntax.
    ///
    /// Inputs:
    /// - Parser cursor positioned at `(`.
    ///
    /// Output:
    /// - `Expr::Fun` when the parenthesized head is followed by `->`.
    /// - The enclosed expression when the source is ordinary grouping.
    ///
    /// Transformation:
    /// - Speculatively parses lambda parameter patterns and rewinds when the
    ///   tokens do not form `(patterns) -> Expr`. This keeps anonymous
    ///   functions expression-shaped without retaining the removed `fun ... end`
    ///   keyword form.
    fn parse_paren_or_lambda_expr(&mut self) -> ParseResult<Expr> {
        let start = self.current().start;
        let checkpoint = self.pos;
        self.expect(TokenKind::LParen)?;
        let mut patterns = Vec::new();
        let lambda_head = if self.check(TokenKind::RParen) {
            self.bump();
            true
        } else {
            loop {
                match self.parse_pattern_with_type_annotation() {
                    Ok(pattern) => patterns.push(pattern),
                    Err(_) => {
                        self.pos = checkpoint;
                        self.expect(TokenKind::LParen)?;
                        let inner = self.parse_expr()?;
                        self.expect(TokenKind::RParen)?;
                        return Ok(inner);
                    }
                }
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
            self.consume_if(TokenKind::RParen)
        };

        if lambda_head && self.consume_if(TokenKind::Arrow) {
            let body = self.parse_expr()?;
            return Ok(Expr::Fun {
                clauses: vec![FunctionClause {
                    patterns,
                    body,
                    span: Span::new(start, self.previous().end),
                    guard: None,
                }],
            });
        }

        self.pos = checkpoint;
        self.expect(TokenKind::LParen)?;
        let inner = self.parse_expr()?;
        self.expect(TokenKind::RParen)?;
        Ok(inner)
    }
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
    fn parse_keyword_expr_clauses(&mut self, stops: &[TokenKind]) -> ParseResult<Vec<CaseClause>> {
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
    fn parse_builtin_block_macro(
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
    fn parse_expr_suffix(&mut self, mut expr: Expr) -> ParseResult<Expr> {
        loop {
            if self.consume_if(TokenKind::Hash) {
                let name = self.expect_ident()?;
                if self.consume_if(TokenKind::LBrace) {
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_expr_field(ExprFieldKind::ErlangRecord)?);
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
                let args = self.parse_expr_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
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
                            && open.kind == TokenKind::LParen
                )
            {
                self.bump();
                let field = self.expect_lower_ident("expected lower-case method name")?;
                self.expect(TokenKind::LParen)?;
                let args = self.parse_expr_list(TokenKind::RParen)?;
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(Expr::FieldAccess {
                        value: Box::new(expr),
                        field,
                    }),
                    args,
                    remote: None,
                    is_fun_value: false,
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
    fn parse_record_expr_field(&mut self, kind: ExprFieldKind) -> ParseResult<MapExprField> {
        let key_token = self.current().clone();
        let valid_key = match kind {
            ExprFieldKind::Map | ExprFieldKind::ErlangRecord => {
                key_token.kind == TokenKind::Atom || key_token.kind == TokenKind::Var
            }
            ExprFieldKind::TerlanRecord => key_token.kind == TokenKind::Atom,
        };
        if !valid_key {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::TerlanRecord => "expected lower-case field name".to_string(),
                    ExprFieldKind::Map | ExprFieldKind::ErlangRecord => {
                        "expected map field key atom".to_string()
                    }
                },
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) && kind == ExprFieldKind::Map {
            false
        } else {
            return Err(ParseError {
                message: match kind {
                    ExprFieldKind::Map => "expected := or => in map expression".to_string(),
                    ExprFieldKind::TerlanRecord | ExprFieldKind::ErlangRecord => {
                        "expected = in struct field".to_string()
                    }
                },
                span: self.current().span(),
            });
        };

        let value = self.parse_expr()?;
        Ok(MapExprField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
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
    fn parse_list_generator(&mut self) -> ParseResult<(Pattern, Expr)> {
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
    fn parse_expr_list(&mut self, end: TokenKind) -> ParseResult<Vec<Expr>> {
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
    fn parse_atom_literal_expr(&mut self) -> ParseResult<Expr> {
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
