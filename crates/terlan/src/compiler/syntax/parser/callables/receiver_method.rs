use super::*;

impl Parser {
    /// Parses a receiver method declaration as a validated preserved form.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the receiver.
    /// - Parser cursor at the receiver opening parenthesis.
    ///
    /// Output:
    /// - A formal `MethodDecl` containing receiver, optional receiver
    ///   mutability, parameters, return type, body clause, visibility, and
    ///   source span.
    ///
    /// Transformation:
    /// - Validates and consumes the receiver-method declaration, then stores it
    ///   as structured parse tree so syntax output, type checking, and backend
    ///   lowering do not have to recover method data from raw source text.
    pub(crate) fn parse_method_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;

        self.expect(TokenKind::LParen)?;
        let receiver_start = self.current().start;
        let receiver_is_mutable = self.consume_keyword("mut");
        let receiver_name = self.expect_lower_ident("expected lower-case method receiver name")?;
        self.expect(TokenKind::Colon)?;
        let receiver_type = self.parse_receiver_type_expr()?;
        let receiver_end = self.previous().end;
        self.expect(TokenKind::RParen)?;

        let name = self.expect_lower_ident("expected lower-case method name")?;
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
        let return_type = self.parse_type_expr(&[TokenKind::Arrow])?;
        self.expect(TokenKind::Arrow)?;
        let body = self.parse_body_expr()?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Method(MethodDecl {
            receiver: Param {
                name: receiver_name,
                annotation: receiver_type,
                is_mutable: receiver_is_mutable,
                default: None,
                span: Span::new(receiver_start, receiver_end),
            },
            name,
            generic_params,
            params,
            return_type,
            is_public,
            generic_bounds,
            clauses: vec![FunctionClause {
                patterns: Vec::new(),
                body,
                span: Span::new(start, self.previous().end),
                guard: None,
            }],
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a bodyless receiver method signature for interface summaries.
    ///
    /// Inputs:
    /// - `is_public`: whether `pub` was consumed before the receiver.
    /// - Parser cursor at the receiver opening parenthesis in interface mode.
    ///
    /// Output:
    /// - A `MethodDecl` with receiver, optional receiver mutability,
    ///   non-receiver params, return type, visibility, and no body clauses.
    ///
    /// Transformation:
    /// - Consumes the same receiver-method header as source methods, but
    ///   terminates at `.` so `.typi` summaries can preserve receiver metadata
    ///   without inventing receiver-first function signatures.
    pub(crate) fn parse_method_signature_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;

        self.expect(TokenKind::LParen)?;
        let receiver_start = self.current().start;
        let receiver_is_mutable = self.consume_keyword("mut");
        let receiver_name = self.expect_lower_ident("expected lower-case method receiver name")?;
        self.expect(TokenKind::Colon)?;
        let receiver_type = self.parse_receiver_type_expr()?;
        let receiver_end = self.previous().end;
        self.expect(TokenKind::RParen)?;

        let name = self.expect_lower_ident("expected lower-case method name")?;
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
        if self.check(TokenKind::Dot) {
            return Err(ParseError {
                message: "expected return type after ':'".to_string(),
                span: self.current().span(),
            });
        }
        let return_type = self.parse_type_expr(&[TokenKind::Dot])?;
        self.expect(TokenKind::Dot)?;

        Ok(Decl::Method(MethodDecl {
            receiver: Param {
                name: receiver_name,
                annotation: receiver_type,
                is_mutable: receiver_is_mutable,
                default: None,
                span: Span::new(receiver_start, receiver_end),
            },
            name,
            generic_params,
            params,
            return_type,
            is_public,
            generic_bounds,
            clauses: Vec::new(),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses the type expression used by a receiver declaration.
    ///
    /// Inputs:
    /// - Parser cursor positioned after `(receiver:`.
    ///
    /// Output:
    /// - A `TypeExpr` preserving the complete receiver type expression.
    ///
    /// Transformation:
    /// - Reuses the canonical type-expression parser so qualified, generic,
    ///   and nested receiver types follow the same grammar as parameters and
    ///   return types.
    fn parse_receiver_type_expr(&mut self) -> ParseResult<TypeExpr> {
        let receiver_type = self.parse_type_expr(&[TokenKind::RParen])?;
        let type_name = receiver_type
            .text
            .split(['[', '(', ' ', '|'])
            .next()
            .unwrap_or_default()
            .rsplit('.')
            .next()
            .unwrap_or_default();
        if !type_name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        {
            return Err(ParseError {
                message: "expected upper-case type name".to_string(),
                span: receiver_type.span,
            });
        }
        Ok(receiver_type)
    }
}
