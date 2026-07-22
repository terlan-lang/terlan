use super::*;
use crate::terlan_syntax::parse_tree::{StringPatternCapture, StringPatternSegment};

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
                if self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|next| next.kind == TokenKind::Dot)
                {
                    return self.parse_qualified_constant_pattern();
                }
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
            TokenKind::Colon => Ok(Pattern::AtomLiteral(self.parse_raw_atom_literal_payload()?)),
            TokenKind::Var => {
                if self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|next| next.kind == TokenKind::Dot)
                {
                    return self.parse_qualified_constant_pattern();
                }
                if self.starts_binary_layout() {
                    return self.parse_binary_layout_pattern();
                }
                if token.text == "Atom"
                    && matches!(
                        self.tokens.get(self.pos + 1),
                        Some(next) if next.kind == TokenKind::LBracket
                    )
                {
                    return Ok(Pattern::AtomLiteral(self.parse_atom_literal_payload()?));
                }
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
                    let mut parts = vec![Pattern::Atom(token.text)];
                    if self.consume_if(TokenKind::RParen) {
                        return Ok(Pattern::Tuple(parts));
                    }
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
                Ok(Pattern::Float(parse_float_literal_token(&token)?))
            }
            TokenKind::String => {
                self.bump();
                if string_pattern_has_elixir_capture(&token.text) {
                    return Err(ParseError {
                        message: string_pattern_diagnostic(&token.text).to_string(),
                        span: token.span(),
                    });
                }
                let Some(value) = parse_string_token_payload(&token.text) else {
                    return Err(ParseError {
                        message: "invalid string literal pattern".to_string(),
                        span: token.span(),
                    });
                };
                if value.contains("${") {
                    return parse_string_capture_pattern(&value, token.span());
                }
                Ok(Pattern::String(value))
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

    fn parse_qualified_constant_pattern(&mut self) -> ParseResult<Pattern> {
        let start = self.current().start;
        let mut segments = vec![self.bump().text.clone()];
        while self.consume_if(TokenKind::Dot) {
            let segment = self.current().clone();
            if !matches!(segment.kind, TokenKind::Atom | TokenKind::Var) {
                return Err(ParseError {
                    message: "expected name after `.` in constant pattern".to_string(),
                    span: segment.span(),
                });
            }
            self.bump();
            segments.push(segment.text);
        }
        let name = segments.join(".");
        let member = segments.last().expect("qualified pattern has a member");
        if segments.len() < 2 || !super::constants::is_screaming_snake_case(member) {
            return Err(ParseError {
                message: "qualified value patterns must end in a SCREAMING_SNAKE_CASE constant"
                    .to_string(),
                span: Span::new(start, self.previous().end),
            });
        }
        Ok(Pattern::Tuple(vec![Pattern::Atom(name)]))
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
        let Some(key) = Self::map_field_key_text(&key_token) else {
            return Err(ParseError {
                message: "invalid keyed field name".to_string(),
                span: key_token.span(),
            });
        };
        Ok(MapField {
            key,
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

/// Returns the stable diagnostic for reserved string-literal pattern syntax.
///
/// Inputs:
/// - Raw string token text, including source delimiters.
///
/// Output:
/// - A stable diagnostic string for the reserved Terlan `${...}` pattern
///   surface or the rejected Elixir-style `#{...}` interpolation spelling.
///
/// Transformation:
/// - Keeps string patterns unavailable until the AST, typechecker, formatter,
///   VM binder, and editor surfaces agree, while still preserving the accepted
///   capture spelling decision.
fn string_pattern_diagnostic(raw: &str) -> &'static str {
    if raw.contains("#{") {
        "string patterns use `${...}` captures; `#{...}` is not Terlan syntax"
    } else {
        "string pattern matching is reserved for Terlan 0.0.7; match with case guards or helper parsers for now"
    }
}

/// Returns whether a string token contains rejected Elixir-style capture syntax.
///
/// Inputs: raw quoted string token text.
/// Output: `true` when the token contains `#{...}` capture syntax.
/// Transformation: keeps the spelling diagnostic independent from Terlan
/// `${...}` parsing so the parser can accept the Terlan capture surface without
/// ever accepting Erlang/Elixir interpolation syntax.
fn string_pattern_has_elixir_capture(raw: &str) -> bool {
    raw.contains("#{")
}

/// Parses one capture-bearing string pattern payload.
///
/// Inputs:
/// - `payload`: decoded string literal payload without outer quotes.
/// - `span`: source span of the original string token.
///
/// Output:
/// - A segmented string pattern preserving literal and capture ordering.
///
/// Transformation:
/// - Scans for deterministic `${name}` and `${name: Type}` capture slots,
///   preserves literal boundaries, and rejects malformed or adjacent captures
///   before type checking and VM matching are implemented.
fn parse_string_capture_pattern(payload: &str, span: Span) -> ParseResult<Pattern> {
    let mut segments = Vec::new();
    let mut rest = payload;
    let mut previous_was_capture = false;

    while let Some(start) = rest.find("${") {
        let literal = &rest[..start];
        if !literal.is_empty() {
            segments.push(StringPatternSegment::Literal(literal.to_string()));
        } else if previous_was_capture {
            return Err(ParseError {
                message: "adjacent string captures require a literal separator".to_string(),
                span,
            });
        }

        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            return Err(ParseError {
                message: "unterminated string capture pattern".to_string(),
                span,
            });
        };
        let capture_source = after_start[..end].trim();
        segments.push(StringPatternSegment::Capture(parse_string_capture_slot(
            capture_source,
            span,
        )?));
        previous_was_capture = true;
        rest = &after_start[end + 1..];
    }

    if !rest.is_empty() {
        segments.push(StringPatternSegment::Literal(rest.to_string()));
    }

    if segments
        .iter()
        .all(|segment| matches!(segment, StringPatternSegment::Literal(_)))
    {
        return Ok(Pattern::String(payload.to_string()));
    }

    Ok(Pattern::StringPattern(segments))
}

/// Parses the payload of one `${...}` string capture slot.
///
/// Inputs:
/// - `source`: capture payload without `${` and `}`.
/// - `span`: source span of the enclosing string token.
///
/// Output:
/// - Capture metadata with optional type annotation text.
///
/// Transformation:
/// - Splits on the first `:` to keep type syntax source-preserving while still
///   validating that capture names and annotations are non-empty.
fn parse_string_capture_slot(source: &str, span: Span) -> ParseResult<StringPatternCapture> {
    if source.is_empty() {
        return Err(ParseError {
            message: "empty string capture pattern".to_string(),
            span,
        });
    }

    let (name, annotation) = match source.split_once(':') {
        Some((name, annotation)) => {
            let annotation = annotation.trim();
            if annotation.is_empty() {
                return Err(ParseError {
                    message: "string capture type annotation cannot be empty".to_string(),
                    span,
                });
            }
            (
                name.trim(),
                Some(TypeExpr {
                    text: annotation.to_string(),
                    span,
                }),
            )
        }
        None => (source.trim(), None),
    };

    if !valid_string_capture_name(name) {
        return Err(ParseError {
            message: "string capture names must be lower-case bindings".to_string(),
            span,
        });
    }

    Ok(StringPatternCapture {
        name: name.to_string(),
        annotation,
    })
}

/// Returns whether a capture name is a canonical lower-case binding.
///
/// Inputs: source name inside `${...}`.
/// Output: `true` for lower-case or underscore-prefixed identifier syntax.
/// Transformation: mirrors ordinary binding-name shape without interpreting
/// capture names as constructor or type names.
fn valid_string_capture_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
