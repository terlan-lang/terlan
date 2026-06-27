use super::constructor_validation::validate_constructor_clause_shapes;
use super::*;

impl Parser {
    /// Parses a constructor declaration.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before `constructor`.
    /// - Parser cursor positioned at the `constructor` keyword.
    ///
    /// Output:
    /// - A structured `ConstructorDecl` with type parameters, clauses,
    ///   visibility, and source span.
    ///
    /// Transformation:
    /// - Consumes the constructor block, validates clause arity/default
    ///   compatibility, and preserves each constructor clause for later
    ///   lowering.
    pub(crate) fn parse_constructor_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Constructor)?;
        let name = self.expect_type_name()?;
        let params = self.parse_optional_type_params()?;
        self.expect(TokenKind::LBrace)?;

        let mut clauses = Vec::new();
        if !self.check(TokenKind::RBrace) {
            loop {
                clauses.push(self.parse_constructor_clause()?);
                if self.consume_if(TokenKind::Semicolon) {
                    if self.check(TokenKind::RBrace) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        validate_constructor_clause_shapes(&clauses)?;

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Constructor(ConstructorDecl {
            name,
            params,
            clauses,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses one constructor clause.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the clause parameter-list opening `(`.
    ///
    /// Output:
    /// - A `ConstructorClause` with parameters, return type, body, and span.
    ///
    /// Transformation:
    /// - Consumes one constructor arm and enforces local default/varargs rules
    ///   before preserving the arm body expression.
    fn parse_constructor_clause(&mut self) -> ParseResult<ConstructorClause> {
        let start = self.current().start;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                let param = self.parse_constructor_param()?;
                let param_is_varargs = param.is_varargs;
                params.push(param);

                if param_is_varargs && !self.check(TokenKind::RParen) {
                    return Err(ParseError {
                        message: "constructor varargs parameter must be last".to_string(),
                        span: self.current().span(),
                    });
                }

                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;

        let has_varargs = params.iter().any(|param| param.is_varargs);
        let has_defaults = params.iter().any(|param| param.default.is_some());
        if has_varargs && has_defaults {
            return Err(ParseError {
                message: "constructor clauses cannot combine defaults and varargs yet".to_string(),
                span: Span::new(start, self.previous().end),
            });
        }

        let mut seen_default = false;
        for param in &params {
            if param.default.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(ParseError {
                    message: "constructor default parameters must be trailing".to_string(),
                    span: param.span,
                });
            }
        }

        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_expr(&[TokenKind::Arrow])?;
        self.expect(TokenKind::Arrow)?;
        let body = self.parse_body_expr_with_clause_sep(None, true)?;

        Ok(ConstructorClause {
            params,
            return_type,
            body,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses one constructor parameter.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a constructor parameter name or `...`.
    ///
    /// Output:
    /// - A `ConstructorParam` with name, type annotation, optional default,
    ///   varargs marker, and span.
    ///
    /// Transformation:
    /// - Consumes constructor parameter syntax and rejects defaults on varargs
    ///   parameters.
    fn parse_constructor_param(&mut self) -> ParseResult<ConstructorParam> {
        let start = self.current().start;
        let is_varargs = self.consume_if(TokenKind::Ellipsis);
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let annotation =
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen, TokenKind::Equals])?;
        let default = if self.consume_if(TokenKind::Equals) {
            if is_varargs {
                return Err(ParseError {
                    message: "constructor varargs parameters cannot have defaults".to_string(),
                    span: Span::new(start, self.previous().end),
                });
            }
            Some(self.parse_single_expr()?)
        } else {
            None
        };

        Ok(ConstructorParam {
            name,
            annotation,
            default,
            is_varargs,
            span: Span::new(start, self.previous().end),
        })
    }
}
