use super::*;

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
        let start = self.current().start;
        let mut depth_p = 0;
        let mut depth_b = 0;
        let mut depth_bra = 0;
        let mut parts = Vec::new();

        while !self.check(TokenKind::EOF) {
            if self.check_any(stop)
                && depth_p == 0
                && depth_b == 0
                && depth_bra == 0
                && !self.is_qualified_type_dot(&parts)
            {
                break;
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
                TokenKind::LBrace => depth_bra += 1,
                TokenKind::RBrace if depth_bra > 0 => depth_bra -= 1,
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
