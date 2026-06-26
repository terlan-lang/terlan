use super::*;

mod postfix;
mod sql;

use sql::parse_sql_interpolations;

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
                if token.text == "sql"
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(next) if next.kind == TokenKind::LBracket
                    ) =>
            {
                self.parse_typed_sql_raw_macro()?
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
                    type_args: Vec::new(),
                    interpolations: Vec::new(),
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
                    let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                    self.expect(TokenKind::RParen)?;
                    Expr::Call {
                        callee: Box::new(Expr::Atom(fun)),
                        type_args: Vec::new(),
                        args,
                        arg_names,
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
                        let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            type_args: Vec::new(),
                            args,
                            arg_names,
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

                    if dotted.len() > 1 && self.call_starts_after_optional_type_args(lookahead) {
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
                            let type_args = self.parse_optional_call_type_args()?;
                            self.expect(TokenKind::LParen)?;
                            let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                            self.expect(TokenKind::RParen)?;
                            return self.parse_expr_suffix(Expr::Call {
                                callee: Box::new(Expr::FieldAccess {
                                    value: Box::new(base_expr),
                                    field,
                                }),
                                type_args,
                                args,
                                arg_names,
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
                                let type_args = self.parse_optional_call_type_args()?;
                                self.expect(TokenKind::LParen)?;
                                let (args, arg_names) =
                                    self.parse_call_arg_list(TokenKind::RParen)?;
                                self.expect(TokenKind::RParen)?;
                                return self.parse_expr_suffix(Expr::Call {
                                    callee: Box::new(Expr::Atom(fun)),
                                    type_args,
                                    args,
                                    arg_names,
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
                        let type_args = self.parse_optional_call_type_args()?;
                        self.expect(TokenKind::LParen)?;
                        let remote = remote_parts.join(".");
                        let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(Expr::Atom(fun)),
                            type_args,
                            args,
                            arg_names,
                            remote: Some(remote),
                            is_fun_value: false,
                        }
                    } else if self.call_starts_after_optional_type_args(self.pos) {
                        let type_args = self.parse_optional_call_type_args()?;
                        self.expect(TokenKind::LParen)?;
                        let (args, arg_names) = self.parse_call_arg_list(TokenKind::RParen)?;
                        self.expect(TokenKind::RParen)?;
                        Expr::Call {
                            callee: Box::new(base_expr),
                            type_args,
                            args,
                            arg_names,
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
                return Err(ParseError {
                    message: "Erlang binary literal syntax is not valid Terlan source; use a normal string literal".to_string(),
                    span: token.span(),
                });
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
                            fields.push(self.parse_record_expr_field(ExprFieldKind::TerlanRecord)?);
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
                    let condition = self.parse_if_condition()?;
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

    /// Parses the dedicated typed SQL raw-macro front door.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the `sql` identifier.
    ///
    /// Output:
    /// - `Expr::RawMacro` named `sql` with one explicit result type argument.
    ///
    /// Transformation:
    /// - Consumes `sql[TypeExpr] { raw sql }`, preserving the raw SQL body for
    ///   the macro/typecheck gate while making the requested row type visible
    ///   to syntax output.
    fn parse_typed_sql_raw_macro(&mut self) -> ParseResult<Expr> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBracket)?;
        let result_type = self.parse_type_expr(&[TokenKind::RBracket])?;
        self.expect(TokenKind::RBracket)?;
        let raw = self.parse_raw_block()?;
        let interpolations = parse_sql_interpolations(&raw, self.previous().span())?;
        Ok(Expr::RawMacro {
            name,
            type_args: vec![result_type],
            interpolations,
            raw,
        })
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

    /// Parses optional explicit type arguments on a call head.
    ///
    /// Inputs:
    /// - Parser cursor positioned either at `[` in `name[Type](...)` or at the
    ///   following `(` when the call has no explicit type arguments.
    ///
    /// Output:
    /// - Parsed type expressions supplied by the call site, or an empty vector
    ///   when the call has no explicit type arguments.
    ///
    /// Transformation:
    /// - Consumes a non-empty bracketed type-expression list and preserves each
    ///   parsed type expression so syntax output can carry generic call metadata
    ///   without encoding it into module or function names.
    pub(super) fn parse_optional_call_type_args(&mut self) -> ParseResult<Vec<TypeExpr>> {
        if !self.consume_if(TokenKind::LBracket) {
            return Ok(Vec::new());
        }

        let mut args = Vec::new();
        if self.check(TokenKind::RBracket) {
            return Err(ParseError {
                message: "call type arguments cannot be empty".to_string(),
                span: self.current().span(),
            });
        }

        loop {
            args.push(self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBracket])?);
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBracket)?;

        Ok(args)
    }

    /// Checks whether a dotted name tail is followed by call syntax.
    ///
    /// Inputs:
    /// - `position`: token index immediately after the dotted name segments.
    ///
    /// Output:
    /// - `true` when the next token begins either an ordinary call or an
    ///   explicit generic call.
    ///
    /// Transformation:
    /// - Performs a narrow syntactic lookahead used only to decide whether a
    ///   dotted expression should enter call parsing; full type-argument
    ///   validation remains in `parse_optional_call_type_args`.
    fn call_starts_after_optional_type_args(&self, position: usize) -> bool {
        match self.tokens.get(position) {
            Some(token) if token.kind == TokenKind::LParen => true,
            Some(token) if token.kind == TokenKind::LBracket => {
                let mut index = position + 1;
                let mut depth = 1i32;
                while let Some(token) = self.tokens.get(index) {
                    match token.kind {
                        TokenKind::LBracket => depth += 1,
                        TokenKind::RBracket => {
                            depth -= 1;
                            if depth == 0 {
                                return matches!(
                                    self.tokens.get(index + 1),
                                    Some(next) if next.kind == TokenKind::LParen
                                );
                            }
                        }
                        TokenKind::EOF => return false,
                        _ => {}
                    }
                    index += 1;
                }
                false
            }
            _ => false,
        }
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
    fn parse_call_arg_list(
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
    fn parse_if_condition(&mut self) -> ParseResult<Expr> {
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
