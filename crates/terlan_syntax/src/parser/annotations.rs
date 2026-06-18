use super::*;

/// Parsed annotation metadata block.
///
/// Inputs:
/// - Source block following an annotation path.
///
/// Output:
/// - Raw preserved block text plus typed entries and standalone values.
///
/// Transformation:
/// - Keeps raw metadata available for compatibility while exposing structured
///   annotation values to syntax output and later semantic validation.
struct ParsedAnnotationBlock {
    raw: String,
    entries: Vec<AnnotationEntry>,
    values: Vec<AnnotationValue>,
}

impl Parser {
    /// Parses declaration-leading annotations that precede the next item.
    ///
    /// Inputs:
    /// - Parser cursor positioned before a possible annotation sequence.
    ///
    /// Output:
    /// - Ordered annotation metadata consumed before the declaration.
    ///
    /// Transformation:
    /// - Recognizes the current formal annotation surface and returns metadata
    ///   for syntax output and later semantic phases.
    pub(super) fn parse_leading_annotations(&mut self) -> ParseResult<Vec<Annotation>> {
        let mut annotations = Vec::new();
        while self.check(TokenKind::At) {
            annotations.push(self.parse_annotation()?);
            self.skip_comments();
        }
        Ok(annotations)
    }

    /// Parses one declaration annotation prefix.
    ///
    /// Inputs:
    /// - Parser cursor at `@`.
    ///
    /// Output:
    /// - Parsed annotation path, optional raw metadata block, and source span.
    ///
    /// Transformation:
    /// - Validates `@lower.path` annotation paths and skips an immediately
    ///   following `{ ... }` metadata block with balanced delimiters. Subject
    ///   values remain a documented follow-up because the token stream does not
    ///   yet carry line boundaries needed to disambiguate them from lower-case
    ///   function declarations.
    fn parse_annotation(&mut self) -> ParseResult<Annotation> {
        let start = self.expect(TokenKind::At)?.start;
        let mut path = vec![self.expect_lower_ident("expected annotation path segment")?];
        while self.consume_if(TokenKind::Dot) {
            path.push(self.expect_lower_ident("expected annotation path segment after '.'")?);
        }

        self.reject_unsupported_annotation_subject()?;

        let (args, entries, values) = if self.check(TokenKind::LBrace) {
            let block = self.parse_annotation_block()?;
            (Some(block.raw), block.entries, block.values)
        } else {
            (None, Vec::new(), Vec::new())
        };

        Ok(Annotation {
            path,
            args,
            entries,
            values,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Rejects annotation subject forms that the current parser can identify.
    ///
    /// Inputs:
    /// - Parser cursor immediately after an annotation path.
    ///
    /// Output:
    /// - `Ok(())` when the next token can be interpreted as declaration syntax
    ///   or annotation metadata.
    /// - A parse error for unambiguous annotation subject syntax.
    ///
    /// Transformation:
    /// - Keeps declaration-leading annotations valid while rejecting subject
    ///   forms from the canonical EBNF until the lexer exposes line-boundary
    ///   information needed to distinguish lower-case subjects from annotated
    ///   function declarations.
    fn reject_unsupported_annotation_subject(&self) -> ParseResult<()> {
        let token = self.current();
        let next = self.tokens.get(self.pos + 1);
        let is_unambiguous_subject = match token.kind {
            TokenKind::Int | TokenKind::Float | TokenKind::String | TokenKind::Var => true,
            TokenKind::Atom => next
                .map(|next| matches!(next.kind, TokenKind::Dot | TokenKind::LBrace))
                .unwrap_or(false),
            _ => false,
        };

        if is_unambiguous_subject {
            return Err(ParseError {
                message: "annotation subjects are not supported in Terlan 0.0.1; place annotations immediately before the declaration".to_string(),
                span: token.span(),
            });
        }

        Ok(())
    }

    /// Parses an annotation metadata block into raw and typed forms.
    ///
    /// Inputs:
    /// - Parser cursor at the opening `{` of an annotation metadata block.
    ///
    /// Output:
    /// - Raw block text and typed entries after consuming the matching `}`.
    ///
    /// Transformation:
    /// - Preserves the previous raw output behavior while also converting
    ///   `key: value` metadata into typed parse tree nodes for semantic validation.
    fn parse_annotation_block(&mut self) -> ParseResult<ParsedAnnotationBlock> {
        let mut parts = Vec::new();
        self.push_expected(TokenKind::LBrace, &mut parts)?;
        let mut entries = Vec::new();
        let mut values = Vec::new();

        if self.check(TokenKind::RBrace) {
            self.push_expected(TokenKind::RBrace, &mut parts)?;
            return Ok(ParsedAnnotationBlock {
                raw: join_parts(&parts),
                entries,
                values,
            });
        }

        loop {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated annotation block".to_string(),
                    span: self.current().span(),
                });
            }

            if self.annotation_item_is_entry() {
                entries.push(self.parse_annotation_entry(&mut parts)?);
            } else {
                values.push(self.parse_annotation_value(&mut parts)?);
            }

            if self.consume_if_push(TokenKind::Semicolon, &mut parts) {
                if self.check(TokenKind::RBrace) {
                    break;
                }
                continue;
            }

            if self.check(TokenKind::RBrace) {
                break;
            }

            return Err(ParseError {
                message: "expected ';' or '}' after annotation entry".to_string(),
                span: self.current().span(),
            });
        }

        self.push_expected(TokenKind::RBrace, &mut parts)?;
        Ok(ParsedAnnotationBlock {
            raw: join_parts(&parts),
            entries,
            values,
        })
    }

    /// Classifies the next top-level annotation block item.
    ///
    /// Inputs:
    /// - Parser cursor at the first token of an annotation block item.
    ///
    /// Output:
    /// - `true` when the next item has `LowerIdent { "." LowerIdent } ":"`.
    ///
    /// Transformation:
    /// - Performs bounded lookahead without consuming tokens so compact
    ///   positional metadata such as `{core.map.new}` can coexist with keyed
    ///   metadata such as `{worker: true}`.
    fn annotation_item_is_entry(&self) -> bool {
        if !matches!(self.current().kind, TokenKind::Atom) {
            return false;
        }

        let mut index = self.pos + 1;
        while matches!(
            (self.tokens.get(index), self.tokens.get(index + 1)),
            (Some(dot), Some(segment))
                if dot.kind == TokenKind::Dot && segment.kind == TokenKind::Atom
        ) {
            index += 2;
        }

        matches!(self.tokens.get(index), Some(token) if token.kind == TokenKind::Colon)
    }

    /// Parses one `key: value` annotation metadata entry.
    ///
    /// Inputs:
    /// - Parser cursor at the first annotation key segment.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - Typed annotation entry with source span.
    ///
    /// Transformation:
    /// - Converts dotted lower-case keys and typed annotation values into the
    ///   private parse tree while preserving every consumed token in `parts`.
    fn parse_annotation_entry(&mut self, parts: &mut Vec<String>) -> ParseResult<AnnotationEntry> {
        let start = self.current().start;
        let key = self.parse_annotation_key(parts)?;
        self.push_expected(TokenKind::Colon, parts)?;
        let value = self.parse_annotation_value(parts)?;
        Ok(AnnotationEntry {
            key,
            value,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses a dotted annotation metadata key.
    ///
    /// Inputs:
    /// - Parser cursor at a lower-case key segment.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - Ordered key path segments.
    ///
    /// Transformation:
    /// - Consumes `LowerIdent { "." LowerIdent }` and mirrors the consumed
    ///   tokens into the raw block accumulator.
    fn parse_annotation_key(&mut self, parts: &mut Vec<String>) -> ParseResult<Vec<String>> {
        let mut key = vec![self.push_lower_ident(parts, "expected annotation key")?];
        while self.consume_if_push(TokenKind::Dot, parts) {
            key.push(self.push_lower_ident(parts, "expected annotation key segment after '.'")?);
        }
        Ok(key)
    }

    /// Parses a typed annotation metadata value.
    ///
    /// Inputs:
    /// - Parser cursor at the start of an annotation value.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - Typed annotation value.
    ///
    /// Transformation:
    /// - Converts primitive, qualified-name, list, and object metadata values
    ///   into parse tree nodes while preserving source token text in `parts`.
    fn parse_annotation_value(&mut self, parts: &mut Vec<String>) -> ParseResult<AnnotationValue> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom if token.text == "true" || token.text == "false" => {
                self.bump();
                parts.push(token.text.clone());
                Ok(AnnotationValue::Bool(token.text == "true"))
            }
            TokenKind::Atom | TokenKind::Var => self.parse_annotation_name_value(parts),
            TokenKind::Int => {
                self.bump();
                parts.push(token.text.clone());
                Ok(AnnotationValue::Int(token.text))
            }
            TokenKind::Float => {
                self.bump();
                parts.push(token.text.clone());
                Ok(AnnotationValue::Float(token.text))
            }
            TokenKind::String => {
                self.bump();
                parts.push(token.text.clone());
                Ok(AnnotationValue::String(token.text))
            }
            TokenKind::LBracket => self.parse_annotation_list_value(parts),
            TokenKind::LBrace => self.parse_annotation_object_value(parts),
            _ => Err(ParseError {
                message: "expected annotation value".to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses a qualified annotation name value.
    ///
    /// Inputs:
    /// - Parser cursor at a lower- or upper-case name segment.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - `AnnotationValue::Name` with ordered path segments.
    ///
    /// Transformation:
    /// - Consumes `NameRef { "." NameRef }` in annotation value position.
    fn parse_annotation_name_value(
        &mut self,
        parts: &mut Vec<String>,
    ) -> ParseResult<AnnotationValue> {
        let mut name = vec![self.push_name_ref(parts, "expected annotation name value")?];
        while self.consume_if_push(TokenKind::Dot, parts) {
            name.push(self.push_name_ref(parts, "expected annotation name segment after '.'")?);
        }
        Ok(AnnotationValue::Name(name))
    }

    /// Parses a list annotation value.
    ///
    /// Inputs:
    /// - Parser cursor at `[`.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - `AnnotationValue::List` preserving value order.
    ///
    /// Transformation:
    /// - Consumes bracketed values separated by commas with an optional trailing
    ///   comma.
    fn parse_annotation_list_value(
        &mut self,
        parts: &mut Vec<String>,
    ) -> ParseResult<AnnotationValue> {
        self.push_expected(TokenKind::LBracket, parts)?;
        let mut values = Vec::new();
        if self.check(TokenKind::RBracket) {
            self.push_expected(TokenKind::RBracket, parts)?;
            return Ok(AnnotationValue::List(values));
        }

        loop {
            values.push(self.parse_annotation_value(parts)?);
            if self.consume_if_push(TokenKind::Comma, parts) {
                if self.check(TokenKind::RBracket) {
                    break;
                }
                continue;
            }
            break;
        }

        self.push_expected(TokenKind::RBracket, parts)?;
        Ok(AnnotationValue::List(values))
    }

    /// Parses an object annotation value.
    ///
    /// Inputs:
    /// - Parser cursor at `{`.
    /// - `parts`: raw output accumulator for the enclosing annotation block.
    ///
    /// Output:
    /// - `AnnotationValue::Object` preserving entry order.
    ///
    /// Transformation:
    /// - Consumes nested annotation entries separated by semicolons with an
    ///   optional trailing semicolon.
    fn parse_annotation_object_value(
        &mut self,
        parts: &mut Vec<String>,
    ) -> ParseResult<AnnotationValue> {
        self.push_expected(TokenKind::LBrace, parts)?;
        let mut entries = Vec::new();
        if self.check(TokenKind::RBrace) {
            self.push_expected(TokenKind::RBrace, parts)?;
            return Ok(AnnotationValue::Object(entries));
        }

        loop {
            entries.push(self.parse_annotation_entry(parts)?);
            if self.consume_if_push(TokenKind::Semicolon, parts) {
                if self.check(TokenKind::RBrace) {
                    break;
                }
                continue;
            }
            if self.check(TokenKind::RBrace) {
                break;
            }
            return Err(ParseError {
                message: "expected ';' or '}' after annotation object entry".to_string(),
                span: self.current().span(),
            });
        }

        self.push_expected(TokenKind::RBrace, parts)?;
        Ok(AnnotationValue::Object(entries))
    }

    /// Consumes an expected token and records its raw text.
    ///
    /// Inputs:
    /// - `kind`: required token kind.
    /// - `parts`: raw annotation block accumulator.
    ///
    /// Output:
    /// - The consumed token.
    ///
    /// Transformation:
    /// - Advances the parser exactly one token and appends its original text to
    ///   the annotation block raw output.
    fn push_expected(&mut self, kind: TokenKind, parts: &mut Vec<String>) -> ParseResult<Token> {
        let token = self.expect(kind)?;
        parts.push(token.text.clone());
        Ok(token)
    }

    /// Optionally consumes a token and records its raw text.
    ///
    /// Inputs:
    /// - `kind`: token kind to consume when present.
    /// - `parts`: raw annotation block accumulator.
    ///
    /// Output:
    /// - `true` when the token was consumed.
    ///
    /// Transformation:
    /// - Advances the parser only when the current token matches `kind`.
    fn consume_if_push(&mut self, kind: TokenKind, parts: &mut Vec<String>) -> bool {
        if self.check(kind) {
            parts.push(self.bump().text);
            true
        } else {
            false
        }
    }

    /// Consumes a lower-case identifier and records its raw text.
    ///
    /// Inputs:
    /// - `parts`: raw annotation block accumulator.
    /// - `message`: diagnostic text for wrong-case identifiers.
    ///
    /// Output:
    /// - Consumed identifier text.
    ///
    /// Transformation:
    /// - Restricts annotation keys to lower-case source identifiers.
    fn push_lower_ident(&mut self, parts: &mut Vec<String>, message: &str) -> ParseResult<String> {
        let token = self.current().clone();
        let ident = self.expect_lower_ident(message)?;
        parts.push(token.text);
        Ok(ident)
    }

    /// Consumes a name reference and records its raw text.
    ///
    /// Inputs:
    /// - `parts`: raw annotation block accumulator.
    /// - `message`: diagnostic text for invalid values.
    ///
    /// Output:
    /// - Consumed name segment text.
    ///
    /// Transformation:
    /// - Accepts lower- and upper-case source names for qualified annotation
    ///   value references.
    fn push_name_ref(&mut self, parts: &mut Vec<String>, message: &str) -> ParseResult<String> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom | TokenKind::Var => {
                self.bump();
                parts.push(token.text.clone());
                Ok(token.text)
            }
            _ => Err(ParseError {
                message: message.to_string(),
                span: token.span(),
            }),
        }
    }

    /// Parses an annotation schema declaration.
    ///
    /// Inputs:
    /// - `is_public`: declaration-site visibility consumed before `annotation`.
    /// - Parser cursor positioned at contextual keyword `annotation`.
    ///
    /// Output:
    /// - Structured `AnnotationSchemaDecl` with annotation path, schema entries,
    ///   visibility, and source span.
    ///
    /// Transformation:
    /// - Consumes `annotation path { ... }.` and preserves each schema entry so
    ///   syntax output can validate user-declared annotation metadata before
    ///   typechecking and backend lowering.
    pub(super) fn parse_annotation_schema_decl(&mut self, is_public: bool) -> ParseResult<Decl> {
        let start = self.current().start;
        self.expect_contextual_keyword("annotation")?;
        let path = self.parse_annotation_schema_path()?;
        self.expect(TokenKind::LBrace)?;

        let mut entries = Vec::new();
        while !self.check(TokenKind::RBrace) {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated annotation schema declaration".to_string(),
                    span: Span::new(start, self.current().end),
                });
            }

            self.skip_comments();
            if self.consume_if(TokenKind::Semicolon) {
                continue;
            }
            if self.check(TokenKind::RBrace) {
                break;
            }

            entries.push(self.parse_annotation_schema_entry()?);
        }

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::Dot)?;
        Ok(Decl::AnnotationSchema(AnnotationSchemaDecl {
            path,
            entries,
            is_public,
            docs: Vec::new(),
            span: Span::new(start, self.previous().end),
        }))
    }

    /// Parses the annotation path used by a schema declaration.
    ///
    /// Inputs:
    /// - Parser cursor at the first annotation path segment.
    ///
    /// Output:
    /// - Ordered lower-case path segments.
    ///
    /// Transformation:
    /// - Consumes `AnnotationName { "." AnnotationName }` using the same
    ///   lower-case source shape as declaration-leading annotations.
    fn parse_annotation_schema_path(&mut self) -> ParseResult<Vec<String>> {
        let mut path = vec![self.expect_lower_ident("expected annotation schema path segment")?];
        while self.consume_if(TokenKind::Dot) {
            path.push(
                self.expect_lower_ident("expected annotation schema path segment after '.'")?,
            );
        }
        Ok(path)
    }

    /// Parses one annotation schema body entry.
    ///
    /// Inputs:
    /// - Parser cursor at `applies_to` or an annotation key.
    ///
    /// Output:
    /// - Structured schema entry.
    ///
    /// Transformation:
    /// - Routes top-level `applies_to` to declaration-target metadata and all
    ///   other lower-case keys to typed key-schema entries.
    fn parse_annotation_schema_entry(&mut self) -> ParseResult<AnnotationSchemaEntry> {
        let start = self.current().start;
        let mut key_parts = Vec::new();
        let key = self.parse_annotation_key(&mut key_parts)?;
        self.expect(TokenKind::Colon)?;

        if key == ["applies_to"] {
            let targets = self.parse_annotation_target_set()?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(AnnotationSchemaEntry::AppliesTo {
                targets,
                span: Span::new(start, self.previous().end),
            });
        }

        let value_type =
            self.parse_annotation_value_type(&[TokenKind::LBrace, TokenKind::Semicolon])?;
        let options = if self.check(TokenKind::LBrace) {
            self.parse_annotation_key_options()?
        } else {
            Vec::new()
        };
        self.expect(TokenKind::Semicolon)?;
        Ok(AnnotationSchemaEntry::Key {
            key,
            value_type,
            options,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Parses an annotation key option block.
    ///
    /// Inputs:
    /// - Parser cursor at the option block opening brace.
    ///
    /// Output:
    /// - Ordered option list.
    ///
    /// Transformation:
    /// - Consumes schema key options separated by semicolons, including an
    ///   optional trailing semicolon.
    fn parse_annotation_key_options(&mut self) -> ParseResult<Vec<AnnotationKeyOption>> {
        self.expect(TokenKind::LBrace)?;
        let mut options = Vec::new();
        while !self.check(TokenKind::RBrace) {
            if self.check(TokenKind::EOF) {
                return Err(ParseError {
                    message: "unterminated annotation key options".to_string(),
                    span: self.current().span(),
                });
            }
            if self.consume_if(TokenKind::Semicolon) {
                continue;
            }
            options.push(self.parse_annotation_key_option()?);
            if self.consume_if(TokenKind::Semicolon) {
                continue;
            }
            if self.check(TokenKind::RBrace) {
                break;
            }
            return Err(ParseError {
                message: "expected ';' or '}' after annotation key option".to_string(),
                span: self.current().span(),
            });
        }
        self.expect(TokenKind::RBrace)?;
        Ok(options)
    }

    /// Parses one annotation key option.
    ///
    /// Inputs:
    /// - Parser cursor at an option name.
    ///
    /// Output:
    /// - Structured key option.
    ///
    /// Transformation:
    /// - Converts supported option keys into typed private parse tree values and rejects
    ///   unknown option keys at parse time.
    fn parse_annotation_key_option(&mut self) -> ParseResult<AnnotationKeyOption> {
        let start = self.current().start;
        let option = self.expect_lower_ident("expected annotation key option")?;
        self.expect(TokenKind::Colon)?;
        match option.as_str() {
            "required" => {
                let value = self.parse_annotation_bool_option_value("required")?;
                Ok(AnnotationKeyOption::Required {
                    value,
                    span: Span::new(start, self.previous().end),
                })
            }
            "repeatable" => {
                let value = self.parse_annotation_bool_option_value("repeatable")?;
                Ok(AnnotationKeyOption::Repeatable {
                    value,
                    span: Span::new(start, self.previous().end),
                })
            }
            "default" => {
                let mut parts = Vec::new();
                let value = self.parse_annotation_value(&mut parts)?;
                Ok(AnnotationKeyOption::Default {
                    value,
                    span: Span::new(start, self.previous().end),
                })
            }
            "applies_to" => {
                let targets = self.parse_annotation_target_set()?;
                Ok(AnnotationKeyOption::AppliesTo {
                    targets,
                    span: Span::new(start, self.previous().end),
                })
            }
            _ => Err(ParseError {
                message: format!("unknown annotation key option `{option}`"),
                span: Span::new(start, self.previous().end),
            }),
        }
    }

    /// Parses a boolean annotation key option value.
    ///
    /// Inputs:
    /// - `option`: option name for diagnostics.
    /// - Parser cursor at a boolean metadata value.
    ///
    /// Output:
    /// - Parsed boolean value.
    ///
    /// Transformation:
    /// - Accepts only canonical `true` or `false` option values.
    fn parse_annotation_bool_option_value(&mut self, option: &str) -> ParseResult<bool> {
        let token = self.current().clone();
        if token.kind == TokenKind::Atom && (token.text == "true" || token.text == "false") {
            self.bump();
            return Ok(token.text == "true");
        }
        Err(ParseError {
            message: format!("annotation key option `{option}` expects Bool"),
            span: token.span(),
        })
    }

    /// Parses a schema target set.
    ///
    /// Inputs:
    /// - Parser cursor at a target name or target list.
    ///
    /// Output:
    /// - Ordered target names such as `function` or `method`.
    ///
    /// Transformation:
    /// - Consumes either one target or a bracketed comma-separated target list.
    fn parse_annotation_target_set(&mut self) -> ParseResult<Vec<String>> {
        if self.consume_if(TokenKind::LBracket) {
            let mut targets = Vec::new();
            if !self.check(TokenKind::RBracket) {
                loop {
                    targets.push(self.parse_annotation_target()?);
                    if self.consume_if(TokenKind::Comma) {
                        if self.check(TokenKind::RBracket) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }
            self.expect(TokenKind::RBracket)?;
            return Ok(targets);
        }

        Ok(vec![self.parse_annotation_target()?])
    }

    /// Parses one annotation target name.
    ///
    /// Inputs:
    /// - Parser cursor at a target token.
    ///
    /// Output:
    /// - Canonical lower-case target name.
    ///
    /// Transformation:
    /// - Accepts the declaration target names listed by the EBNF and rejects
    ///   unsupported target spellings immediately.
    fn parse_annotation_target(&mut self) -> ParseResult<String> {
        let token = self.current().clone();
        let target = match token.kind {
            TokenKind::Import
            | TokenKind::Type
            | TokenKind::Struct
            | TokenKind::Constructor
            | TokenKind::Trait
            | TokenKind::Impl
            | TokenKind::Template => token.text.clone(),
            TokenKind::Atom => token.text.clone(),
            _ => {
                return Err(ParseError {
                    message: "expected annotation declaration target".to_string(),
                    span: token.span(),
                });
            }
        };

        if !matches!(
            target.as_str(),
            "module"
                | "import"
                | "type"
                | "opaque_type"
                | "struct"
                | "constructor"
                | "trait"
                | "impl"
                | "function"
                | "method"
                | "template"
                | "config"
        ) {
            return Err(ParseError {
                message: format!("unknown annotation declaration target `{target}`"),
                span: token.span(),
            });
        }

        self.bump();
        Ok(target)
    }

    /// Parses an annotation value type.
    ///
    /// Inputs:
    /// - `terminators`: tokens that end the value type in the current context.
    /// - Parser cursor at the value type.
    ///
    /// Output:
    /// - `AnnotationValueType` preserving normalized source type text.
    ///
    /// Transformation:
    /// - Parses list and object annotation value types recursively, and reuses
    ///   the formal type-expression parser for primitive, named, and qualified
    ///   value types.
    fn parse_annotation_value_type(
        &mut self,
        terminators: &[TokenKind],
    ) -> ParseResult<AnnotationValueType> {
        if self.consume_if(TokenKind::LBracket) {
            let inner = self.parse_annotation_value_type(&[TokenKind::RBracket])?;
            self.expect(TokenKind::RBracket)?;
            return Ok(AnnotationValueType {
                text: format!("[{}]", inner.text),
            });
        }

        if self.consume_if(TokenKind::LBrace) {
            let mut entries = Vec::new();
            while !self.check(TokenKind::RBrace) {
                if self.check(TokenKind::EOF) {
                    return Err(ParseError {
                        message: "unterminated annotation object value type".to_string(),
                        span: self.current().span(),
                    });
                }
                let mut key_parts = Vec::new();
                let key = self.parse_annotation_key(&mut key_parts)?.join(".");
                self.expect(TokenKind::Colon)?;
                let value_type =
                    self.parse_annotation_value_type(&[TokenKind::Semicolon, TokenKind::RBrace])?;
                entries.push(format!("{key}: {}", value_type.text));
                if self.consume_if(TokenKind::Semicolon) {
                    continue;
                }
                if self.check(TokenKind::RBrace) {
                    break;
                }
                return Err(ParseError {
                    message: "expected ';' or '}' after annotation object value type entry"
                        .to_string(),
                    span: self.current().span(),
                });
            }
            self.expect(TokenKind::RBrace)?;
            return Ok(AnnotationValueType {
                text: format!("{{{}}}", entries.join("; ")),
            });
        }

        let mut stop = terminators.to_vec();
        if !stop.contains(&TokenKind::Comma) {
            stop.push(TokenKind::Comma);
        }
        let ty = self.parse_type_expr(&stop)?;
        Ok(AnnotationValueType { text: ty.text })
    }
}
