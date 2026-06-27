use super::*;

#[path = "callables/constructor.rs"]
mod constructor;
mod constructor_validation;
#[path = "callables/receiver_method.rs"]
mod receiver_method;

impl Parser {
    /// Parses optional square-bracket type parameters.
    ///
    /// Inputs:
    /// - Parser cursor positioned at `[` or the next declaration token.
    ///
    /// Output:
    /// - Preserved type-parameter texts, or an empty list when absent.
    ///
    /// Transformation:
    /// - Consumes `[T, U]`-style declaration parameters using shared
    ///   type-expression parsing for each parameter slot.
    pub(super) fn parse_optional_type_params(&mut self) -> ParseResult<Vec<String>> {
        let mut params = Vec::new();
        if self.consume_if(TokenKind::LBracket) {
            if !self.check(TokenKind::RBracket) {
                loop {
                    params.push(self.parse_type_param_text()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RBracket)?;
        }
        Ok(params)
    }
    /// Parses a bodyless function signature.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the function name.
    /// - `is_macro`: whether `macro` was consumed before the function name.
    /// - Parser cursor positioned at the lower-case function name.
    ///
    /// Output:
    /// - A `FunctionDecl` with params, return type, generic bounds, and no
    ///   body clauses.
    ///
    /// Transformation:
    /// - Consumes interface-style callable syntax ending at `.` so summaries
    ///   can preserve public function surfaces without source bodies.
    pub(super) fn parse_function_signature_decl(
        &mut self,
        is_public: bool,
        is_macro: bool,
    ) -> ParseResult<Decl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case function name")?;
        let generic_params = self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        self.validate_param_defaults_trailing(&params)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);

        self.expect(TokenKind::Colon)?;
        if self.check(TokenKind::Arrow) || self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }

        let return_type = self.parse_type_expr(&[TokenKind::Dot])?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Function(FunctionDecl {
            name,
            generic_params,
            params,
            return_type,
            is_public,
            is_macro,
            generic_bounds,
            clauses: Vec::new(),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }
    /// Parses a source function declaration.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the function name.
    /// - `is_macro`: whether `macro` was consumed before the function name.
    /// - Parser cursor positioned at the lower-case function name.
    ///
    /// Output:
    /// - A `FunctionDecl` with typed params or clause patterns, return type,
    ///   generic bounds, body clauses, visibility, and span.
    ///
    /// Transformation:
    /// - Distinguishes typed function heads from pattern-clause heads and
    ///   normalizes both source shapes into one function declaration model.
    pub(super) fn parse_function_decl(
        &mut self,
        is_public: bool,
        is_macro: bool,
    ) -> ParseResult<Decl> {
        let start = self.current().start;
        let name = self.expect_lower_ident("expected lower-case function name")?;
        let generic_params = self.consume_generic_params_if_present()?;
        let mut generic_bounds = self.consume_angle_generic_params_if_present()?;

        self.expect(TokenKind::LParen)?;
        if !self.check(TokenKind::RParen) && !self.is_typed_param_start() {
            return self.parse_untyped_function_decl_after_name(
                start,
                name,
                generic_params,
                is_public,
                is_macro,
                generic_bounds,
            );
        }
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                params.push(self.parse_param()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        self.validate_param_defaults_trailing(&params)?;
        generic_bounds.extend(self.consume_constraint_list_if_present()?);

        self.expect(TokenKind::Colon)?;
        if self.check(TokenKind::Arrow) || self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }

        let return_type = self.parse_type_expr(&[TokenKind::Arrow, TokenKind::Dot])?;

        let mut clauses = Vec::new();

        let consumed_arrow = self.consume_if(TokenKind::Arrow);
        if consumed_arrow {
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            self.expect(TokenKind::Dot)?;
            clauses.push(FunctionClause {
                patterns: params
                    .iter()
                    .map(|param| Pattern::Var(param.name.clone()))
                    .collect(),
                body,
                guard: None,
                span: Span::new(start, self.previous().end),
            });

            return Ok(Decl::Function(FunctionDecl {
                name,
                generic_params: generic_params.clone(),
                params,
                return_type,
                is_public,
                is_macro,
                generic_bounds: generic_bounds.clone(),
                clauses,
                docs: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }

        self.expect(TokenKind::Dot)?;

        loop {
            let clause_name = self.expect_lower_ident("expected lower-case function name")?;
            if clause_name != name {
                return Err(ParseError {
                    message: "expected function clause for declared function name".to_string(),
                    span: self.previous().span(),
                });
            }
            self.expect(TokenKind::LParen)?;

            let mut clause_patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    clause_patterns.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            clauses.push(FunctionClause {
                patterns: clause_patterns,
                body,
                guard: None,
                span: Span::new(start, self.previous().end),
            });

            if self.consume_if(TokenKind::Semicolon) {
                if self.check(TokenKind::EOF) || self.check(TokenKind::Dot) {
                    break;
                }
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        Ok(Decl::Function(FunctionDecl {
            name,
            generic_params,
            params,
            return_type,
            is_public,
            is_macro,
            generic_bounds,
            clauses,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }
    /// Parses a pattern-clause function after its first name and `(`.
    ///
    /// Inputs:
    /// - `start`: source start offset for the function group.
    /// - `name`: already-consumed function name.
    /// - `generic_params`: source generic parameters parsed after the name.
    /// - `is_public`: declaration-site visibility.
    /// - `is_macro`: macro declaration marker.
    /// - `generic_bounds`: bounds parsed before the first clause parameter.
    ///
    /// Output:
    /// - A `FunctionDecl` with dynamic placeholder params and parsed clauses.
    ///
    /// Transformation:
    /// - Preserves classic pattern-matched function clauses while deriving a
    ///   dynamic parameter surface from the clause arity.
    fn parse_untyped_function_decl_after_name(
        &mut self,
        start: usize,
        name: String,
        generic_params: Vec<String>,
        is_public: bool,
        is_macro: bool,
        generic_bounds: Vec<String>,
    ) -> ParseResult<Decl> {
        let mut clauses = Vec::new();
        let mut arity = None;

        loop {
            let clause_start = if clauses.is_empty() {
                start
            } else {
                self.current().start
            };
            let mut patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    patterns.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;

            if let Some(expected) = arity {
                if patterns.len() != expected {
                    return Err(ParseError {
                        message: format!(
                            "clause for {name} has arity {}, expected {expected}",
                            patterns.len()
                        ),
                        span: self.current().span(),
                    });
                }
            } else {
                arity = Some(patterns.len());
            }

            if self.consume_if(TokenKind::Colon) {
                self.parse_type_expr(&[TokenKind::Arrow])?;
            }

            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name.as_str()), false)?;
            clauses.push(FunctionClause {
                patterns,
                body,
                guard,
                span: Span::new(clause_start, self.previous().end),
            });

            if self.consume_if(TokenKind::Semicolon) {
                let clause_name = self.expect_lower_ident("expected lower-case function name")?;
                if clause_name != name {
                    return Err(ParseError {
                        message: "expected function clause for declared function name".to_string(),
                        span: self.previous().span(),
                    });
                }
                self.expect(TokenKind::LParen)?;
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        let arity = arity.unwrap_or(0);
        Ok(Decl::Function(FunctionDecl {
            name,
            generic_params,
            params: (0..arity)
                .map(|index| Param {
                    name: format!("_Arg{}", index + 1),
                    annotation: TypeExpr {
                        text: "Dynamic".to_string(),
                        span: Span::new(start, start),
                    },
                    is_mutable: false,
                    default: None,
                    span: Span::new(start, start),
                })
                .collect(),
            return_type: TypeExpr {
                text: "Dynamic".to_string(),
                span: Span::new(start, start),
            },
            is_public,
            is_macro,
            generic_bounds,
            clauses,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }
    /// Parses additional clauses for a pending function declaration.
    ///
    /// Inputs:
    /// - `name`: function name that every clause must repeat.
    /// - `arity`: expected clause pattern count.
    /// - Parser cursor positioned at the next clause name.
    ///
    /// Output:
    /// - Ordered function clauses.
    ///
    /// Transformation:
    /// - Consumes repeated `name(patterns) -> body` clauses and enforces
    ///   stable name/arity across the group.
    pub(super) fn parse_function_clause_group(
        &mut self,
        name: &str,
        arity: usize,
    ) -> ParseResult<Vec<FunctionClause>> {
        let mut clauses = Vec::new();

        loop {
            let start = self.current().start;
            let clause_name = self.expect_lower_ident("expected lower-case function name")?;
            if clause_name != name {
                return Err(ParseError {
                    message: "expected function clause for declared function name".to_string(),
                    span: self.previous().span(),
                });
            }

            self.expect(TokenKind::LParen)?;
            let mut patterns = Vec::new();
            if !self.check(TokenKind::RParen) {
                loop {
                    patterns.push(self.parse_pattern_with_type_annotation()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;

            if patterns.len() != arity {
                return Err(ParseError {
                    message: format!(
                        "clause for {name} has arity {}, expected {arity}",
                        patterns.len()
                    ),
                    span: self.current().span(),
                });
            }

            let guard = if self.consume_if(TokenKind::When) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };

            self.expect(TokenKind::Arrow)?;
            let body = self.parse_body_expr_with_clause_sep(Some(name), false)?;
            clauses.push(FunctionClause {
                patterns,
                body,
                span: Span::new(start, self.previous().end),
                guard,
            });

            if self.consume_if(TokenKind::Semicolon) {
                if self.check(TokenKind::EOF) {
                    break;
                }
                continue;
            }

            self.expect(TokenKind::Dot)?;
            break;
        }

        Ok(clauses)
    }
    /// Parses one typed callable parameter.
    ///
    /// Inputs:
    /// - Parser cursor positioned at an optional `mut` marker or parameter
    ///   name.
    ///
    /// Output:
    /// - A `Param` with name, type annotation, mutability, optional default,
    ///   and span.
    ///
    /// Transformation:
    /// - Consumes `mut name: Type`, `name: Type`, or `name: Type = expr` and
    ///   rejects currently unsupported function varargs syntax.
    pub(super) fn parse_param(&mut self) -> ParseResult<Param> {
        let start = self.current().start;
        if self.consume_if(TokenKind::Ellipsis) {
            return Err(ParseError {
                message: "function varargs parameters are not supported in Terlan 0.0.1"
                    .to_string(),
                span: Span::new(start, self.previous().end),
            });
        }
        let is_mutable = self.consume_keyword("mut");
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let annotation = if self.check(TokenKind::RParen) {
            TypeExpr {
                text: "Dynamic".to_string(),
                span: Span::new(start, start),
            }
        } else {
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen, TokenKind::Equals])?
        };
        let default = if self.consume_if(TokenKind::Equals) {
            Some(self.parse_single_expr()?)
        } else {
            None
        };

        Ok(Param {
            name,
            annotation,
            is_mutable,
            default,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Validates default-parameter ordering for function-like parameters.
    ///
    /// Inputs:
    /// - `params`: parsed callable parameters in source order.
    ///
    /// Output:
    /// - `Ok(())` when all defaulted parameters are trailing.
    /// - `Err(ParseError)` anchored to the first required parameter after a
    ///   default.
    ///
    /// Transformation:
    /// - Scans left to right, records when a default appears, and rejects a
    ///   later required parameter so call-site arity remains deterministic.
    pub(super) fn validate_param_defaults_trailing(&self, params: &[Param]) -> ParseResult<()> {
        let mut seen_default = false;
        for param in params {
            if param.default.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(ParseError {
                    message: "default parameters must be trailing".to_string(),
                    span: param.span,
                });
            }
        }
        Ok(())
    }
    /// Reports whether the current cursor starts a typed parameter.
    ///
    /// Inputs:
    /// - Parser cursor inside a function parameter list.
    ///
    /// Output:
    /// - `true` when the next tokens have typed-parameter shape.
    ///
    /// Transformation:
    /// - Performs bounded lookahead so the function parser can distinguish
    ///   typed parameter lists from pattern-clause parameter lists.
    fn is_typed_param_start(&self) -> bool {
        if self.check(TokenKind::Ellipsis) {
            return true;
        }
        if !matches!(self.current().kind, TokenKind::Atom | TokenKind::Var) {
            return false;
        }
        matches!(
            self.tokens.get(self.pos + 1),
            Some(token) if token.kind == TokenKind::Colon
        )
    }
    /// Consumes square-bracket generic syntax when present.
    ///
    /// Inputs:
    /// - Parser cursor positioned at `[` or the next callable token.
    ///
    /// Output:
    /// - Preserved generic parameter texts, or an empty list when absent.
    ///
    /// Transformation:
    /// - Reuses declaration type-parameter parsing so function generics and
    ///   type generics preserve the same HKT and variance surface syntax.
    pub(super) fn consume_generic_params_if_present(&mut self) -> ParseResult<Vec<String>> {
        self.parse_optional_type_params()
    }
    /// Consumes angle-bracket callable constraints when present.
    ///
    /// Inputs:
    /// - Parser cursor positioned at `<` or the next callable token.
    ///
    /// Output:
    /// - Preserved bound texts, or an empty list when absent.
    ///
    /// Transformation:
    /// - Parses balanced angle-bound syntax, rejects runtime-expression tokens
    ///   in type position, and returns each top-level bound as canonical text.
    pub(super) fn consume_angle_generic_params_if_present(&mut self) -> ParseResult<Vec<String>> {
        if !self.consume_if(TokenKind::Lt) {
            return Ok(Vec::new());
        }

        let start = self.previous().start;
        let mut depth = 1usize;
        let mut depth_p = 0usize;
        let mut depth_b = 0usize;
        let mut depth_br = 0usize;
        let mut current = Vec::new();
        let mut bounds = Vec::new();

        let flush = |current: &mut Vec<String>, bounds: &mut Vec<String>| {
            let bound = join_parts(current);
            if !bound.trim().is_empty() {
                bounds.push(bound);
            }
            current.clear();
        };

        while !self.check(TokenKind::EOF) {
            if self.consume_if(TokenKind::Lt) {
                depth += 1;
                current.push("<".to_string());
                continue;
            }
            if self.consume_if(TokenKind::Gt) {
                depth -= 1;
                if depth == 0 {
                    flush(&mut current, &mut bounds);
                    return Ok(bounds);
                }
                current.push(">".to_string());
                continue;
            }

            if self.check(TokenKind::Comma)
                && depth == 1
                && depth_p == 0
                && depth_b == 0
                && depth_br == 0
            {
                flush(&mut current, &mut bounds);
                self.bump();
                continue;
            }

            let token = self.bump();
            if matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::DocBlockComment
            ) {
                continue;
            }
            if matches!(
                token.kind,
                TokenKind::Case
                    | TokenKind::Of
                    | TokenKind::End
                    | TokenKind::Fun
                    | TokenKind::When
                    | TokenKind::And
                    | TokenKind::Or
                    | TokenKind::PipeForward
                    | TokenKind::EqEq
                    | TokenKind::EqEqEq
                    | TokenKind::NotEq
                    | TokenKind::NotEqEq
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::DivRem
                    | TokenKind::Rem
                    | TokenKind::Bang
            ) {
                return Err(ParseError {
                    message: format!(
                        "runtime expression token '{}' is not valid in type position",
                        token.text
                    ),
                    span: token.span(),
                });
            }

            match token.kind {
                TokenKind::LParen => depth_p += 1,
                TokenKind::RParen => depth_p = depth_p.saturating_sub(1),
                TokenKind::LBracket => depth_b += 1,
                TokenKind::RBracket => depth_b = depth_b.saturating_sub(1),
                TokenKind::LBrace => depth_br += 1,
                TokenKind::RBrace => depth_br = depth_br.saturating_sub(1),
                _ => {}
            }
            current.push(token.text);
        }

        Err(ParseError {
            message: "unterminated generic parameter list".to_string(),
            span: Span::new(start, self.current().end),
        })
    }
    /// Parses a canonical post-parameter trait constraint list.
    ///
    /// Inputs:
    /// - Parser cursor immediately after a callable parameter list.
    ///
    /// Output:
    /// - Constraint type-expression texts such as `Eq[A]` or `Show[A]`, or an
    ///   empty list when no constraint list is present.
    ///
    /// Transformation:
    /// - Consumes `[Constraint, ...]` only in callable constraint position and
    ///   preserves each constraint as type-expression text for later semantic
    ///   conversion into typechecker `FunctionBound` values.
    pub(super) fn consume_constraint_list_if_present(&mut self) -> ParseResult<Vec<String>> {
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
    /// Parses a declaration body expression.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the first body expression token.
    ///
    /// Output:
    /// - A single expression or sequence expression for semicolon-separated
    ///   body forms.
    ///
    /// Transformation:
    /// - Delegates to body parsing without clause-separator lookahead.
    pub(super) fn parse_body_expr(&mut self) -> ParseResult<Expr> {
        self.parse_body_expr_with_clause_sep(None, false)
    }

    /// Parses a declaration body expression with optional clause separation.
    ///
    /// Inputs:
    /// - `clause_name`: function name that starts the next clause, when body
    ///   parsing occurs inside a function group.
    /// - `is_constructor_clause`: whether `(` starts the next constructor arm.
    /// - Parser cursor positioned at the first body expression token.
    ///
    /// Output:
    /// - Parsed body expression, possibly wrapped as `Expr::Sequence`.
    ///
    /// Transformation:
    /// - Consumes semicolon-separated body expressions until it reaches a
    ///   token sequence that belongs to the next function/constructor clause.
    fn parse_body_expr_with_clause_sep(
        &mut self,
        clause_name: Option<&str>,
        is_constructor_clause: bool,
    ) -> ParseResult<Expr> {
        let mut expr = self.parse_single_expr()?;
        while self.consume_if(TokenKind::Equals) {
            expr = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
        }

        while self.consume_if(TokenKind::Comma) {
            let rest = self.parse_body_expr_with_clause_sep(clause_name, is_constructor_clause)?;
            expr = rest;
        }

        let mut expressions = Vec::new();
        while self.check(TokenKind::Semicolon) {
            if self.is_clause_separator_ahead(clause_name, is_constructor_clause) {
                break;
            }

            self.bump();
            expressions.push(self.parse_single_expr()?);
        }

        if !expressions.is_empty() {
            let mut values = vec![expr];
            values.append(&mut expressions);
            expr = Expr::Sequence(values);
        }
        Ok(expr)
    }
    /// Reports whether the current semicolon introduces the next clause.
    ///
    /// Inputs:
    /// - `clause_name`: expected repeated function name, when parsing
    ///   function clauses.
    /// - `is_constructor_clause`: whether constructor clause syntax is active.
    ///
    /// Output:
    /// - `true` when the current semicolon should stop body parsing.
    ///
    /// Transformation:
    /// - Performs non-consuming lookahead for function and constructor clause
    ///   boundaries.
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
