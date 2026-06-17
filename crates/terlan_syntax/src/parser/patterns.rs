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
    ///   literal, list, map, record, tuple, or parenthesized pattern forms.
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
                if self.check(TokenKind::LParen)
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
                    if self.consume_if(TokenKind::RBrace) {
                        return Ok(Pattern::Map(Vec::new()));
                    }

                    let mut fields = Vec::new();
                    loop {
                        fields.push(self.parse_pattern_map_field()?);
                        if !self.consume_if(TokenKind::Comma) {
                            break;
                        }
                    }

                    self.expect(TokenKind::RBrace)?;
                    return Ok(Pattern::Map(fields));
                }

                let name = self.expect_ident()?;
                self.expect(TokenKind::LBrace)?;
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

                Ok(Pattern::Record { name, fields })
            }
            TokenKind::LBrace => {
                self.bump();
                if self.check(TokenKind::RBrace) {
                    self.bump();
                    return Ok(Pattern::Tuple(Vec::new()));
                }
                let mut items = Vec::new();
                loop {
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
    /// - A `MapField` with required/exact-match metadata and nested pattern.
    ///
    /// Transformation:
    /// - Converts `key = pattern` and `key => pattern` pattern fields into the
    ///   shared map-field representation used by downstream phases.
    fn parse_pattern_map_field(&mut self) -> ParseResult<MapField> {
        let key_token = self.current().clone();
        if key_token.kind != TokenKind::Atom && key_token.kind != TokenKind::Var {
            return Err(ParseError {
                message: "expected map field key atom".to_string(),
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) {
            false
        } else {
            return Err(ParseError {
                message: "expected := or => in map pattern".to_string(),
                span: self.current().span(),
            });
        };

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
    }

    /// Parses one record pattern field.
    ///
    /// Inputs:
    /// - Parser cursor positioned at a record-pattern key inside `Type { ... }`.
    ///
    /// Output:
    /// - A `MapField` preserving field name, nested pattern, and required flag.
    ///
    /// Transformation:
    /// - Consumes record pattern field syntax and reuses the map-field payload
    ///   shape so record and map matching can share later lowering code.
    fn parse_record_pattern_field(&mut self) -> ParseResult<MapField> {
        let key_token = self.current().clone();
        if key_token.kind != TokenKind::Atom && key_token.kind != TokenKind::Var {
            return Err(ParseError {
                message: "expected record field key atom".to_string(),
                span: key_token.span(),
            });
        }

        self.bump();
        let required = if self.consume_if(TokenKind::Equals) {
            true
        } else if self.consume_if(TokenKind::FatArrow) {
            false
        } else {
            return Err(ParseError {
                message: "expected = in record pattern".to_string(),
                span: self.current().span(),
            });
        };

        let value = self.parse_pattern()?;
        Ok(MapField {
            key: key_token.text,
            value: Box::new(value),
            required,
        })
    }
}
