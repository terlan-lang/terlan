use super::*;

impl Parser {
    pub(super) fn parse_constant_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Const)?;
        if self.current().kind == TokenKind::Atom {
            return self.parse_const_function(start, is_public);
        }
        let name_token = self.current().clone();
        if !matches!(name_token.kind, TokenKind::Var) {
            return Err(ParseError {
                message: "constant name must use SCREAMING_SNAKE_CASE".to_string(),
                span: name_token.span(),
            });
        }
        self.bump();
        if !is_screaming_snake_case(&name_token.text) {
            return Err(ParseError {
                message: format!(
                    "constant `{}` must use SCREAMING_SNAKE_CASE",
                    name_token.text
                ),
                span: name_token.span(),
            });
        }
        self.expect(TokenKind::Colon)?;
        let annotation = self.parse_type_expr(&[TokenKind::Equals])?;
        self.expect(TokenKind::Equals)?;
        let value = self.parse_body_expr()?;
        self.expect(TokenKind::Dot)?;
        Ok(Decl::Constant(ConstantDecl {
            name: name_token.text,
            annotation,
            value,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    fn parse_const_function(&mut self, start: usize, is_public: bool) -> ParseResult<Decl> {
        let name = self.expect_lower_ident("expected lower-case const function name")?;
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
        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_expr(&[TokenKind::Arrow])?;
        self.expect(TokenKind::Arrow)?;
        let body = self.parse_body_expr()?;
        self.expect(TokenKind::Dot)?;
        Ok(Decl::ConstFunction(ConstFunctionDecl {
            name,
            params,
            return_type,
            body,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    pub(super) fn parse_constant_interface_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        self.parse_constant_decl(is_public)
    }

    pub(super) fn parse_associated_const(&mut self) -> ParseResult<TraitConstDecl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Const)?;
        let name = self.parse_constant_name("trait associated constant")?;
        self.expect(TokenKind::Colon)?;
        let annotation = self.parse_type_expr(&[TokenKind::Equals, TokenKind::Dot])?;
        let default = if self.consume_if(TokenKind::Equals) {
            Some(self.parse_body_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Dot)?;
        Ok(TraitConstDecl {
            name,
            annotation,
            default,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        })
    }

    pub(super) fn parse_impl_const(&mut self) -> ParseResult<ImplConstDecl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Const)?;
        let name = self.parse_constant_name("trait implementation constant")?;
        let annotation = if self.consume_if(TokenKind::Colon) {
            Some(self.parse_type_expr(&[TokenKind::Equals])?)
        } else {
            None
        };
        self.expect(TokenKind::Equals)?;
        let value = self.parse_body_expr()?;
        self.expect(TokenKind::Dot)?;
        Ok(ImplConstDecl {
            name,
            annotation,
            value,
            span: Span::new(start, self.previous().end),
        })
    }

    fn parse_constant_name(&mut self, owner: &str) -> ParseResult<String> {
        let token = self.current().clone();
        if token.kind != TokenKind::Var || !is_screaming_snake_case(&token.text) {
            return Err(ParseError {
                message: format!("{owner} name must use SCREAMING_SNAKE_CASE"),
                span: token.span(),
            });
        }
        self.bump();
        Ok(token.text)
    }
}

pub(super) fn is_screaming_snake_case(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('_')
        && !name.ends_with('_')
        && !name.contains("__")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        && name.bytes().any(|byte| byte.is_ascii_uppercase())
}
