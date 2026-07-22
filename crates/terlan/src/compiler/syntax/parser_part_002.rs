
impl Parser {
    /// Creates a parser cursor over a token stream.
    ///
    /// Inputs:
    /// - `tokens`: lexer output terminated by EOF.
    ///
    /// Output:
    /// - Parser positioned at the first token.
    ///
    /// Transformation:
    /// - Stores tokens without modification and initializes the cursor.
    fn new(tokens: Vec<Token>, let_binding_mode: LetBindingMode) -> Self {
        Self {
            tokens,
            pos: 0,
            let_binding_mode,
            implicit_let_binding_offsets: Vec::new(),
        }
    }

    /// Parses a public source declaration after consuming no tokens yet.
    ///
    /// Inputs:
    /// - Parser cursor at `pub`.
    ///
    /// Output:
    /// - Parsed declaration with public visibility.
    ///
    /// Transformation:
    /// - Consumes `pub` and dispatches to the declaration parser for the next
    ///   keyword or function head.
    fn parse_pub_decl(&mut self) -> ParseResult<Decl> {
        let pub_start = self.current().start;
        self.expect_keyword(TokenKind::Pub)?;
        if self.looks_like_reserved_shape_synonym_decl() {
            return self.parse_shape_synonym_raw_decl(true, pub_start);
        }
        match self.current().kind {
            TokenKind::Const => self.parse_constant_decl(true),
            TokenKind::Type => self.parse_type_decl(false, true),
            TokenKind::Opaque => self.parse_type_decl(true, true),
            TokenKind::Struct => self.parse_struct_decl(true),
            TokenKind::Constructor => self.parse_constructor_decl(true),
            TokenKind::Trait => self.parse_trait_decl(true),
            TokenKind::Impl => self.parse_trait_impl_decl(true),
            TokenKind::LParen => self.parse_method_decl(true),
            TokenKind::Atom if self.current().text == "annotation" => {
                self.parse_annotation_schema_decl(true)
            }
            TokenKind::Macro => {
                self.bump();
                self.parse_function_decl(true, true)
            }
            TokenKind::Atom | TokenKind::Var => self.parse_function_decl(true, false),
            _ => Err(ParseError {
                message: "expected declaration after `pub`".to_string(),
                span: self.current().span(),
            }),
        }
    }

    /// Parses a public interface declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `pub` inside an interface module.
    ///
    /// Output:
    /// - Parsed public interface declaration or signature.
    ///
    /// Transformation:
    /// - Consumes `pub` and dispatches to interface-aware declaration parsers.
    fn parse_pub_interface_decl(&mut self) -> ParseResult<Decl> {
        let pub_start = self.current().start;
        self.expect_keyword(TokenKind::Pub)?;
        if self.looks_like_reserved_shape_synonym_decl() {
            return self.parse_shape_synonym_raw_decl(true, pub_start);
        }
        match self.current().kind {
            TokenKind::Const => self.parse_constant_interface_decl(true),
            TokenKind::Type => self.parse_type_interface_decl(false, true),
            TokenKind::Opaque => self.parse_type_interface_decl(true, true),
            TokenKind::Struct => self.parse_struct_decl(true),
            TokenKind::Constructor => self.parse_constructor_decl(true),
            TokenKind::Trait => self.parse_trait_decl(true),
            TokenKind::Impl => self.parse_trait_impl_interface_decl(true),
            TokenKind::LParen => self.parse_method_signature_decl(true),
            TokenKind::Atom if self.current().text == "annotation" => {
                self.parse_annotation_schema_decl(true)
            }
            TokenKind::Macro => {
                self.bump();
                if self.interface_macro_has_body() {
                    self.parse_function_decl(true, true)
                } else {
                    self.parse_function_signature_decl(true, true)
                }
            }
            TokenKind::Atom | TokenKind::Var => self.parse_function_signature_decl(true, false),
            _ => Err(ParseError {
                message: "expected declaration after `pub`".to_string(),
                span: self.current().span(),
            }),
        }
    }

    /// Detects whether a public interface macro carries exported evaluator IR.
    fn interface_macro_has_body(&self) -> bool {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace => brace_depth = brace_depth.saturating_sub(1),
                TokenKind::Arrow if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    return true;
                }
                TokenKind::Dot if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    return false;
                }
                TokenKind::EOF => return false,
                _ => {}
            }
        }
        false
    }

    /// Parses a shape synonym as a structured reserved declaration.
    ///
    /// Inputs:
    /// - Parser cursor at the potential contextual `shape` declaration head.
    ///
    /// - `is_public`: whether the caller already consumed `pub`.
    /// - `span_start`: source start for the resulting declaration span.
    ///
    /// Output:
    /// - A structured `shape` declaration that later compiler phases still
    ///   reject until shape expansion is implemented.
    ///
    /// Transformation:
    /// - Keeps the agreed `shape Name(...) = ...` syntax parse-preserved while
    ///   preventing it from being mistaken for a callable declaration.
    fn parse_shape_synonym_raw_decl(
        &mut self,
        is_public: bool,
        span_start: usize,
    ) -> ParseResult<Decl> {
        let Some(name_token) = self.tokens.get(self.pos + 1) else {
            return Err(ParseError {
                message: "shape synonym names must be upper-case".to_string(),
                span: self.current().span(),
            });
        };
        if name_token.kind != TokenKind::Var {
            return Err(ParseError {
                message: "shape synonym names must be upper-case".to_string(),
                span: name_token.span(),
            });
        }

        let name = name_token.text.clone();
        let mut raw = self.parse_shape_synonym_raw_payload(span_start)?;
        let params = Self::parse_shape_synonym_params(&raw.text);
        let (body, guard) = Self::parse_shape_synonym_body_and_guard(&raw.text);
        if is_public {
            raw.text = format!("pub {}", raw.text);
            raw.span = Span::new(span_start, raw.span.end);
        }
        Ok(Decl::Shape(ShapeDecl {
            name,
            params,
            body,
            guard,
            text: raw.text,
            docs: Vec::new(),
            is_public,
            span: raw.span,
        }))
    }

    /// Parses one raw shape-synonym payload until its declaration period.
    ///
    /// Inputs:
    /// - `span_start`: source start used by the caller.
    ///
    /// Output:
    /// - Raw declaration payload for `shape`.
    ///
    /// Transformation:
    /// - Unlike generic raw declarations, structural braces inside the shape
    ///   body do not end the declaration. Only a top-level period terminates
    ///   the shape, which allows guarded pattern bodies such as
    ///   `{status, body} where status in 200..299`.
    fn parse_shape_synonym_raw_payload(
        &mut self,
        span_start: usize,
    ) -> ParseResult<UnsupportedDecl> {
        let mut parts = Vec::new();
        let mut brace_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut found_dot = false;

        while !self.check(TokenKind::EOF) {
            if self.check(TokenKind::Dot)
                && brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
            {
                self.bump();
                found_dot = true;
                break;
            }

            match self.current().kind {
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                }
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => {
                    paren_depth = paren_depth.saturating_sub(1);
                }
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                }
                _ => {}
            }
            parts.push(self.bump().text);
        }

        if !found_dot {
            return Err(ParseError {
                message: "unterminated shape declaration".to_string(),
                span: Span::new(span_start, self.current().end),
            });
        }

        Ok(UnsupportedDecl {
            kind: "shape".to_string(),
            text: parts.join(" "),
            docs: Vec::new(),
            span: Span::new(span_start, self.previous().end),
        })
    }

    /// Extracts shape parameter names from preserved declaration text.
    ///
    /// Inputs: raw shape declaration text beginning with `shape Name`.
    /// Output: parameter names declared in the optional shape head.
    /// Transformation: scans only the head before `=` so body parentheses do
    /// not affect parser-level metadata.
    fn parse_shape_synonym_params(text: &str) -> Vec<String> {
        let head = text.split_once('=').map_or(text, |(head, _)| head);
        let Some(open_index) = head.find('(') else {
            return Vec::new();
        };
        let Some(close_index) = head[open_index + 1..].find(')') else {
            return Vec::new();
        };
        head[open_index + 1..open_index + 1 + close_index]
            .split(',')
            .map(str::trim)
            .filter(|param| !param.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Splits preserved shape text into body and optional guard text.
    ///
    /// Inputs: raw shape declaration text beginning with `shape Name`.
    /// Output: source-shaped body text and optional `where`/`when` guard.
    /// Transformation: keeps this parser layer textual; type-aware pattern and
    /// guard expansion belongs to the later shape-expansion phase.
    fn parse_shape_synonym_body_and_guard(text: &str) -> (String, Option<String>) {
        let body = text
            .split_once('=')
            .map_or("", |(_, body)| body)
            .trim()
            .to_string();
        for marker in [" where ", " when "] {
            if let Some(index) = body.find(marker) {
                return (
                    body[..index].trim().to_string(),
                    Some(body[index + marker.len()..].trim().to_string()),
                );
            }
        }
        (body, None)
    }

    /// Returns whether the current cursor points at the reserved shape-synonym
    /// declaration surface.
    ///
    /// Inputs:
    /// - Parser cursor at a contextual `shape` atom candidate.
    ///
    /// Output:
    /// - `true` when the following tokens match a future shape-synonym
    ///   declaration head.
    ///
    /// Transformation:
    /// - Distinguishes `shape Name(...) = ...` and malformed lower-case
    ///   variants from a legal ordinary function named `shape`.
    fn looks_like_reserved_shape_synonym_decl(&self) -> bool {
        if self.current().kind != TokenKind::Atom || self.current().text != "shape" {
            return false;
        }

        let Some(name) = self.tokens.get(self.pos + 1) else {
            return false;
        };
        if !matches!(name.kind, TokenKind::Var | TokenKind::Atom) {
            return false;
        }
        self.shape_synonym_head_uses_equals(self.pos + 2)
    }

    /// Returns whether a candidate shape declaration head is followed by `=`.
    ///
    /// Inputs:
    /// - Token index immediately after the candidate shape name.
    ///
    /// Output:
    /// - `true` for `shape Name = ...` and `shape Name(...) = ...`.
    ///
    /// Transformation:
    /// - Scans only the declaration head. This keeps the old `=>` spelling
    ///   available for future implication syntax instead of consuming it as a
    ///   shape-synonym marker.
    fn shape_synonym_head_uses_equals(&self, head_start: usize) -> bool {
        let Some(token) = self.tokens.get(head_start) else {
            return false;
        };
        if token.kind == TokenKind::Equals {
            return true;
        }
        if token.kind != TokenKind::LParen {
            return false;
        }
        let mut depth = 0usize;
        for index in head_start..self.tokens.len() {
            match self.tokens[index].kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self
                            .tokens
                            .get(index + 1)
                            .is_some_and(|next| next.kind == TokenKind::Equals);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Parses a template declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `template`.
    ///
    /// Output:
    /// - Parsed template declaration.
    ///
    /// Transformation:
    /// - Consumes the template header, source path, typed props, and terminating
    ///   dot into a `TemplateDecl`.
    fn parse_template_decl(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Template)?;
        let name = self.expect_type_name()?;
        if !self.consume_keyword("from") {
            return Err(ParseError {
                message: "expected `from` in template declaration".to_string(),
                span: self.current().span(),
            });
        }
        let raw_path = self.expect(TokenKind::String)?.text.clone();
        let source_path = raw_path
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(&raw_path)
            .to_string();
        self.expect(TokenKind::LBrace)?;

        let mut props = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                self.skip_comments();
                let _docs = self.take_item_docs();
                self.skip_comments();
                let prop_start = self.current().start;
                let prop_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let annotation = self.parse_type_expr(&[
                    TokenKind::Comma,
                    TokenKind::RBrace,
                    TokenKind::Equals,
                ])?;
                let default = if self.consume_if(TokenKind::Equals) {
                    Some(self.parse_single_expr()?)
                } else {
                    None
                };
                props.push(TemplatePropDecl {
                    name: prop_name,
                    annotation,
                    default,
                    span: Span::new(prop_start, self.previous().end),
                });

                if self.consume_if(TokenKind::Comma) {
                    continue;
                }
                break;
            }
            self.validate_template_prop_defaults_trailing(&props)?;

            self.expect(TokenKind::RBrace)?;
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Template(TemplateDecl {
            name,
            source_path,
            props,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Reports whether the cursor starts a template declaration.
    ///
    /// Inputs:
    /// - Current parser cursor.
    ///
    /// Output:
    /// - `true` when the next tokens match `template Name from`.
    ///
    /// Transformation:
    /// - Peeks ahead without advancing.
    fn is_template_decl_start(&self) -> bool {
        self.current().text == "template"
            && matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if matches!(token.kind, TokenKind::Atom | TokenKind::Var)
            )
            && matches!(
                self.tokens.get(self.pos + 2),
                Some(token) if token.text == "from"
            )
    }

    /// Validates default-property ordering for generated template signatures.
    ///
    /// Inputs:
    /// - `props`: parsed template properties in declaration order.
    ///
    /// Output:
    /// - `Ok(())` when all defaulted template properties are trailing.
    /// - `Err(ParseError)` anchored at the first required property after a
    ///   default.
    ///
    /// Transformation:
    /// - Treats template declarations as generated callable signatures for
    ///   0.0.5 named/default argument semantics.
    fn validate_template_prop_defaults_trailing(
        &self,
        props: &[TemplatePropDecl],
    ) -> ParseResult<()> {
        let mut seen_default = false;
        for prop in props {
            if prop.default.is_some() {
                seen_default = true;
            } else if seen_default {
                return Err(ParseError {
                    message: "template default properties must be trailing".to_string(),
                    span: prop.span,
                });
            }
        }
        Ok(())
    }

    /// Parses a raw unsupported declaration block.
    ///
    /// Inputs:
    /// - `kind`: raw declaration kind selected by the caller.
    ///
    /// Output:
    /// - Unsupported declaration preserving raw text for downstream diagnostics.
    ///
    /// Transformation:
    /// - Consumes nested braces until the declaration terminator and joins the
    ///   token text into a stable raw declaration payload.
    fn parse_raw_decl(&mut self, kind: String) -> ParseResult<Decl> {
        let start = self.current().start;
        let mut parts = vec![kind.clone()];
        if self.current().text == kind {
            self.bump();
        }

        let mut depth = if self.consume_if(TokenKind::LBrace) {
            parts.push("{".to_string());
            1
        } else {
            0
        };

        let mut found_dot = false;

        while !self.check(TokenKind::EOF) {
            if self.consume_if(TokenKind::Dot) {
                if depth == 0 {
                    found_dot = true;
                    break;
                }
                parts.push(".".to_string());
                continue;
            }

            if self.consume_if(TokenKind::LBrace) {
                depth += 1;
                parts.push("{".to_string());
                continue;
            }

            if self.consume_if(TokenKind::RBrace) {
                if depth == 0 {
                    return Err(ParseError {
                        message: format!("unterminated {} declaration", kind),
                        span: Span::new(start, self.current().end),
                    });
                }
                depth -= 1;
                parts.push("}".to_string());
                if depth == 0 {
                    if self.check(TokenKind::Dot) {
                        self.bump();
                    }
                    found_dot = true;
                    break;
                }
                continue;
            }

            parts.push(self.bump().text);
        }

        if parts.is_empty() {
            return Err(ParseError {
                message: format!("malformed {} declaration", kind),
                span: Span::new(start, self.current().end),
            });
        }

        if !found_dot {
            return Err(ParseError {
                message: format!("unterminated {} declaration", kind),
                span: Span::new(start, self.current().end),
            });
        }

        Ok(Decl::Raw(UnsupportedDecl {
            kind,
            text: parts.join(" "),
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses an interface export declaration.
    ///
    /// Inputs:
    /// - Parser cursor at `export` inside an interface module.
    ///
    /// Output:
    /// - Parsed export declaration.
    ///
    /// Transformation:
    /// - Accepts type export lists or function arity exports and consumes the
    ///   terminating dot.
    fn parse_interface_export(&mut self) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_keyword(TokenKind::Export)?;
        if self.consume_keyword("type") {
            if self.consume_if(TokenKind::LParen) {
                loop {
                    self.expect_ident()?;
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    break;
                }
                self.expect(TokenKind::RParen)?;
            } else {
                loop {
                    self.expect_ident()?;
                    if self.consume_if(TokenKind::Comma) {
                        continue;
                    }
                    break;
                }
            }

            self.expect(TokenKind::Dot)?;
            return Ok(Decl::Export(ExportDecl {
                items: Vec::new(),
                span: Span::new(start, self.previous().end),
            }));
        }

        let mut items = Vec::new();
        loop {
            let name = self.expect_ident()?;
            if !self.consume_if(TokenKind::Slash) {
                return Err(ParseError {
                    message: "expected function arity in interface export".to_string(),
                    span: self.current().span(),
                });
            }

            let arity = {
                self.expect(TokenKind::Int)?;
                self.previous()
                    .text
                    .parse::<usize>()
                    .map_err(|_| ParseError {
                        message: "expected numeric arity".to_string(),
                        span: self.previous().span(),
                    })?
            };

            items.push(ExportItem {
                name,
                arity,
                span: Span::new(self.previous().start, self.previous().end),
            });

            if self.consume_if(TokenKind::Comma) {
                continue;
            }
            break;
        }

        self.expect(TokenKind::Dot)?;
        Ok(Decl::Export(ExportDecl {
            items,
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses a canonical type-like declaration name.
    ///
    /// Inputs: the parser cursor must point at a `TypeName` position in a
    /// declaration head.
    /// Outputs: the type name text or a syntax diagnostic at the offending
    /// token.
    /// Transformation: consumes only lexer `Var` tokens, which represent
    /// upper-case identifiers in Terlan source mode.
    fn expect_type_name(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Var => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Atom => Err(ParseError {
                message: "expected upper-case type name".to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected type name".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a lower-case source identifier for a grammar position that
    /// explicitly requires `LowerIdent`.
    ///
    /// Inputs: the parser cursor must point at the expected lower-case
    /// identifier, and `message` describes the grammar position for diagnostics.
    /// Outputs: the identifier text or a syntax diagnostic at the offending
    /// token.
    /// Transformation: consumes only lexer `Atom` tokens, preserving the source
    /// spelling of the lower-case identifier.
    fn expect_lower_ident(&mut self, message: &str) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                Ok(token.text)
            }
            TokenKind::Var => Err(ParseError {
                message: message.to_string(),
                span: token.span(),
            }),
            _ => Err(ParseError {
                message: "expected lower-case identifier".to_string(),
                span: token.span(),
            }),
        }
    }

    fn parse_raw_block(&mut self) -> ParseResult<String> {
        let start = self.current().start;
        self.expect(TokenKind::LBrace)?;

        let mut depth = 1usize;
        let mut raw = String::new();
        let mut previous_end = start + 1;
        while !self.check(TokenKind::EOF) {
            let token = self.bump();
            if token.start > previous_end {
                raw.push(' ');
            }
            match token.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    raw.push_str(&token.text);
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(raw);
                    }
                    raw.push_str(&token.text);
                }
                _ => raw.push_str(&token.text),
            }
            previous_end = token.end;
        }

        Err(ParseError {
            message: "unterminated html block".to_string(),
            span: Span::new(start, self.previous().end),
        })
    }
}
