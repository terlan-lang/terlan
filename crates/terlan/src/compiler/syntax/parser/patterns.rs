use super::*;

impl Parser {
    /// Parses a function-clause pattern and discards an optional source type
    /// annotation.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a clause parameter pattern.
    ///
    /// Output:
    /// - The parsed pattern, with any `: Type` clause annotation consumed.
    ///
    /// Transformation:
    /// - Keeps source-level typed pattern syntax parseable while preserving the
    ///   existing parse tree shape, where pattern type annotations are enforced
    ///   by later phases rather than represented on `Pattern`.
    pub(super) fn parse_pattern_with_type_annotation(&mut self) -> ParseResult<Pattern> {
        let pattern = self.parse_pattern()?;
        if self.consume_if(TokenKind::Colon) {
            self.parse_type_expr(&[TokenKind::Comma, TokenKind::RParen])?;
        }
        Ok(pattern)
    }

    /// Parses a canonical Terlan pattern.
    ///
    /// Inputs:
    /// - Parser cursor positioned at the first token of a pattern.
    ///
    /// Output:
    /// - A parse tree pattern for wildcard, binding, atom, constructor-like,
    ///   literal, list, map, struct, tuple, or parenthesized pattern forms.
    ///
    /// Transformation:
    /// - Consumes exactly one pattern form, recursively consuming nested
    ///   pattern elements and preserving diagnostics for invalid constructor
    ///   and collection pattern shapes.
    pub(super) fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Atom => {
                self.bump();
                if token.text == "_" {
                    Ok(Pattern::Wildcard)
                } else {
                    if self.check(TokenKind::LParen) {
                        return Err(ParseError {
                            message: "lowercase bindings cannot be used as constructor patterns"
                                .to_string(),
                            span: token.span(),
                        });
                    }
                    Ok(Pattern::Var(token.text))
                }
            }
            TokenKind::Colon => {
                self.bump();
                let atom = self.expect_atom_literal_name()?;
                Ok(Pattern::Atom(atom))
            }
            TokenKind::Var => {
                self.bump();
                if self.check(TokenKind::LBrace)
                    && token
                        .text
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    self.bump();
                    let mut fields = Vec::new();
                    if !self.consume_if(TokenKind::RBrace) {
                        loop {
                            fields.push(self.parse_record_pattern_field()?);
                            if !self.consume_if(TokenKind::Comma) {
                                break;
                            }
                        }

                        self.expect(TokenKind::RBrace)?;
                    }

                    Ok(Pattern::Record {
                        name: token.text,
                        fields,
                    })
                } else if self.check(TokenKind::LParen)
                    && token
                        .text
                        .chars()
                        .next()
                        .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    self.bump();
                    if self.check(TokenKind::RParen) {
                        return Err(ParseError {
                            message: "constructor patterns require at least one argument"
                                .to_string(),
                            span: token.span(),
                        });
                    }
                    let mut parts = vec![Pattern::Atom(token.text)];
                    loop {
                        parts.push(self.parse_pattern()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Pattern::Tuple(parts))
                } else if token
                    .text
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_uppercase())
                {
                    Ok(Pattern::Tuple(vec![Pattern::Atom(token.text)]))
                } else {
                    Ok(Pattern::Var(token.text))
                }
            }
            TokenKind::Int => {
                self.bump();
                Ok(Pattern::Int(parse_int_literal_token(&token)?))
            }
            TokenKind::Float => {
                self.bump();
                Ok(Pattern::Float(token.text.parse::<f64>().unwrap_or(0.0)))
            }
            TokenKind::LBracket => {
                self.bump();
                if self.check(TokenKind::RBracket) {
                    self.bump();
                    return Ok(Pattern::List(Vec::new()));
                }

                let first = self.parse_pattern()?;
                if self.consume_if(TokenKind::Pipe) {
                    let tail = self.parse_pattern()?;
                    self.expect(TokenKind::RBracket)?;
                    return Ok(Pattern::ListCons(Box::new(first), Box::new(tail)));
                }

                let mut items = vec![first];
                while self.consume_if(TokenKind::Comma) {
                    if self.check(TokenKind::RBracket) {
                        break;
                    }
                    items.push(self.parse_pattern()?);
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Pattern::List(items))
            }
            TokenKind::Hash => {
                self.bump();
                if self.consume_if(TokenKind::LBrace) {
                    return Err(ParseError {
                        message: "anonymous keyed patterns use `{field: pattern}` syntax"
                            .to_string(),
                        span: token.span(),
                    });
                }

                Err(ParseError {
                    message: "struct patterns must use Type { field: pattern } syntax".to_string(),
                    span: token.span(),
                })
            }
            TokenKind::LBrace => {
                self.bump();
                if self.check(TokenKind::RBrace) {
                    self.bump();
                    return Ok(Pattern::Map(Vec::new()));
                }
                if self.keyed_pattern_field_starts() {
                    let fields = self.parse_keyed_pattern_fields()?;
                    self.expect(TokenKind::RBrace)?;
                    return Ok(Pattern::Map(fields));
                }

                let first = self.parse_pattern()?;
                if !self.consume_if(TokenKind::Comma) {
                    return Err(ParseError {
                        message: "tuple patterns require at least two positional values"
                            .to_string(),
                        span: self.current().span(),
                    });
                }

                let mut items = vec![first];
                loop {
                    if self.check(TokenKind::RBrace) {
                        break;
                    }
                    items.push(self.parse_pattern()?);
                    if !self.consume_if(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace)?;
                Ok(Pattern::Tuple(items))
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_pattern()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment => {
                self.bump();
                self.parse_pattern()
            }
            TokenKind::DocBlockComment => {
                self.bump();
                self.parse_pattern()
            }
            _ => Err(ParseError {
                message: format!("unexpected token {:?} in pattern", token.kind),
                span: token.span(),
            }),
        }
    }

    /// Parses one map pattern field.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a map-pattern key inside `#{ ... }`.
    ///
    /// Output:
    /// - A `MapField` with typed-map destructuring metadata and nested pattern.
    ///
    /// Transformation:
    /// - Converts `key: pattern` pattern fields into the shared map-field
    ///   representation used by downstream phases.
    fn parse_pattern_map_field(&mut self) -> ParseResult<MapField> {
        let key_token = self.current().clone();
        if key_token.kind != TokenKind::Atom && key_token.kind != TokenKind::String {
            return Err(ParseError {
                message: "expected keyed field name".to_string(),
                span: key_token.span(),
            });
        }

        self.bump();
        self.expect(TokenKind::Colon)?;

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: key_token.text,
            value: Box::new(value),
            required: true,
        })
    }

    /// Parses one struct pattern field.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a struct-pattern key inside `Type { ... }`.
    ///
    /// Output:
    /// - A `MapField` preserving field name, nested pattern, and required flag.
    ///
    /// Transformation:
    /// - Consumes struct destructuring syntax and reuses the map-field payload
    ///   shape so struct and map matching can share later lowering code.
    fn parse_record_pattern_field(&mut self) -> ParseResult<MapField> {
        let key = self.parse_record_field_key("expected struct field key")?;
        self.expect(TokenKind::Colon)?;

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: Self::field_key_text(&key),
            value: Box::new(value),
            required: true,
        })
    }

    /// Reports whether the current token starts a keyed pattern field.
    ///
    /// Inputs: parser cursor inside a brace-delimited pattern.
    /// Output: true for `name:` and `"name":` field starts.
    /// Transformation: uses local lookahead only; full key validation happens
    /// in `parse_pattern_map_field`.
    fn keyed_pattern_field_starts(&self) -> bool {
        matches!(self.current().kind, TokenKind::Atom | TokenKind::String)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| token.kind == TokenKind::Colon)
    }

    /// Parses comma-separated keyed pattern fields inside `{ ... }`.
    ///
    /// Inputs: cursor at the first field key.
    /// Output: map/keyed-container pattern fields.
    /// Transformation: delegates field syntax to `parse_pattern_map_field` and
    /// permits a trailing comma before the closing brace.
    fn parse_keyed_pattern_fields(&mut self) -> ParseResult<Vec<MapField>> {
        let mut fields = Vec::new();
        loop {
            fields.push(self.parse_pattern_map_field()?);
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
            if self.check(TokenKind::RBrace) {
                break;
            }
        }
        Ok(fields)
    }
}
