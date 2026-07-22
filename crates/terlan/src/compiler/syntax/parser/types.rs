use super::*;
use crate::terlan_syntax::{
    AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC, DUPLICATE_RECORD_TYPE_FIELD_DIAGNOSTIC,
};
use std::collections::HashSet;

impl Parser {
    /// Parses preserved type-expression text.
    ///
    /// Inputs:
    /// - `stop`: token kinds that terminate the type expression at top-level
    ///   nesting depth.
    /// - Parser cursor positioned at the first type token.
    ///
    /// Output:
    /// - A `TypeExpr` containing normalized source text and span.
    ///
    /// Transformation:
    /// - Consumes tokens until a top-level stop token, while preserving nested
    ///   delimiters, comments are ignored, qualified dotted names stay intact,
    ///   and obvious runtime-expression tokens are rejected from type position.
    pub(super) fn parse_type_expr(&mut self, stop: &[TokenKind]) -> ParseResult<TypeExpr> {
        self.parse_type_expr_inner(stop, DUPLICATE_RECORD_TYPE_FIELD_DIAGNOSTIC)
    }

    /// Parses an implication field type while rejecting ambiguous nested shapes.
    pub(super) fn parse_implication_type_expr(
        &mut self,
        stop: &[TokenKind],
    ) -> ParseResult<TypeExpr> {
        self.parse_type_expr_inner(stop, AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC)
    }

    fn parse_type_expr_inner(
        &mut self,
        stop: &[TokenKind],
        duplicate_record_field_diagnostic: &'static str,
    ) -> ParseResult<TypeExpr> {
        let start = self.current().start;
        let mut depth_p = 0;
        let mut depth_b = 0;
        let mut depth_bra = 0;
        let mut parts = Vec::new();
        let mut record_field_scopes = Vec::<HashSet<String>>::new();

        while !self.check(TokenKind::EOF) {
            if self.check_any(stop)
                && depth_p == 0
                && depth_b == 0
                && depth_bra == 0
                && !self.is_qualified_type_dot(&parts)
                && !self.is_existential_type_dot(&parts)
            {
                break;
            }
            if self.check(TokenKind::Colon)
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| matches!(token.kind, TokenKind::Atom | TokenKind::String))
                && !self.is_record_type_field_colon(depth_bra)
            {
                let payload = self.parse_raw_atom_literal_payload()?;
                parts.push(format!(
                    "Atom[{}]",
                    crate::terlan_syntax::quoted_string_literal(&payload)
                ));
                continue;
            }
            if self.check(TokenKind::Colon) && self.is_record_type_field_colon(depth_bra) {
                let field = self.tokens[self.pos - 1].clone();
                let field_span = field.span();
                let scope = record_field_scopes
                    .last_mut()
                    .expect("record field colon requires an open record scope");
                if !scope.insert(field.text) {
                    return Err(ParseError {
                        message: duplicate_record_field_diagnostic.to_string(),
                        span: field_span,
                    });
                }
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

            match token.kind {
                TokenKind::LParen => depth_p += 1,
                TokenKind::RParen if depth_p > 0 => depth_p -= 1,
                TokenKind::LBracket => depth_b += 1,
                TokenKind::RBracket if depth_b > 0 => depth_b -= 1,
                TokenKind::LBrace => {
                    depth_bra += 1;
                    record_field_scopes.push(HashSet::new());
                }
                TokenKind::RBrace if depth_bra > 0 => {
                    depth_bra -= 1;
                    record_field_scopes.pop();
                }
                _ => {}
            }
            parts.push(token.text);
        }

        if parts.is_empty() {
            return Err(ParseError {
                message: "expected type".to_string(),
                span: Span::new(start, self.current().end),
            });
        }

        let text = join_parts(&parts);
        if text.contains("=>") {
            return Err(ParseError {
                message:
                    "`=>` implication constraints are only valid in generic parameter constraints"
                        .to_string(),
                span: Span::new(start, self.previous().end),
            });
        }
        if let Some(token) = invalid_runtime_type_token(&text) {
            return Err(ParseError {
                message: format!(
                    "runtime expression token '{token}' is not valid in type position"
                ),
                span: Span::new(start, self.previous().end),
            });
        }

        Ok(TypeExpr {
            text,
            span: Span::new(start, self.previous().end),
        })
    }

    /// Reports whether the current colon separates a record field from its type.
    fn is_record_type_field_colon(&self, brace_depth: usize) -> bool {
        brace_depth > 0
            && self
                .tokens
                .get(self.pos.saturating_sub(1))
                .is_some_and(|token| is_type_field_name(&token.text))
    }

    /// Reports whether the current dot belongs to a qualified type reference.
    ///
    /// Inputs:
    /// - `parts`: already-collected type-expression token texts.
    ///
    /// Output:
    /// - `true` when the current `.` is tightly surrounded by identifier-like
    ///   tokens and should not terminate type parsing.
    ///
    /// Transformation:
    /// - Performs non-consuming token-boundary checks so stop-token logic can
    ///   distinguish `module.Type` from declaration terminators.
    fn is_qualified_type_dot(&self, parts: &[String]) -> bool {
        if !self.check(TokenKind::Dot) || parts.is_empty() {
            return false;
        }
        let previous = self.tokens.get(self.pos.saturating_sub(1));
        let current = self.current();
        let next = self.tokens.get(self.pos + 1);

        match (previous, next) {
            (Some(previous), Some(next)) => {
                previous.end == current.start
                    && next.start == current.end
                    && is_identifier_like_token(&previous.kind)
                    && is_identifier_like_token(&next.kind)
            }
            _ => false,
        }
    }

    /// Reports whether the current dot separates existential binders from body.
    ///
    /// Inputs:
    /// - `parts`: already-collected type-expression token texts.
    ///
    /// Output:
    /// - `true` when the current `.` belongs to `exists T. Body` instead of
    ///   terminating the surrounding declaration.
    ///
    /// Transformation:
    /// - Recognizes the restricted existential binder prefix while rejecting
    ///   later dots after the body has started, so declaration terminators keep
    ///   their normal meaning.
    fn is_existential_type_dot(&self, parts: &[String]) -> bool {
        if !self.check(TokenKind::Dot)
            || parts.first().map(String::as_str) != Some("exists")
            || parts.iter().any(|part| part == ".")
            || parts.len() < 2
        {
            return false;
        }

        parts[1..]
            .iter()
            .enumerate()
            .all(|(index, part)| match index % 2 {
                0 => is_upper_type_identifier(part),
                _ => part == ",",
            })
    }
}

/// Reports whether text can name a structural record field.
fn is_type_field_name(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Validates a source type-variable name inside an existential binder.
///
/// Inputs:
/// - `text`: candidate binder token text.
///
/// Output:
/// - `true` when the token is an uppercase Terlan type identifier.
///
/// Transformation:
/// - Keeps the syntax-level existential recognizer aligned with ordinary type
///   variable spelling without allocating semantic type variables here.
fn is_upper_type_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Finds runtime-expression tokens that are invalid in type text.
///
/// Inputs:
/// - `input`: normalized type-expression text.
///
/// Output:
/// - The first invalid runtime token found, if any.
///
/// Transformation:
/// - Scans symbolic operators by substring and alphabetic operators by word
///   boundary to produce a precise type-position diagnostic.
fn invalid_runtime_type_token(input: &str) -> Option<&'static str> {
    const INVALID: &[&str] = &[
        "case", "if", "when", "and", "&&", "or", "||", "not", "|>", "==", "!=", "=:=", "/=", "=/=",
        "*", "/", "div", "rem", "!",
    ];

    for token in INVALID {
        if token.chars().all(|ch| ch.is_ascii_alphabetic()) {
            if input
                .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .any(|word| word == *token)
            {
                return Some(token);
            }
        } else if input.contains(token) {
            return Some(token);
        }
    }
    None
}
