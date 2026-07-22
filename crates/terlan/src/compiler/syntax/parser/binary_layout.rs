use std::collections::BTreeSet;

use super::*;

impl Parser {
    /// Reports whether the cursor starts a descriptor-backed binary layout.
    pub(super) fn starts_binary_layout(&self) -> bool {
        self.current().kind == TokenKind::Var
            && self.current().text == "Binary"
            && matches!(
                self.tokens.get(self.pos + 1),
                Some(token) if token.kind == TokenKind::LBracket
            )
    }

    /// Parses a descriptor-backed binary layout expression.
    pub(super) fn parse_binary_layout_expr(&mut self) -> ParseResult<Expr> {
        let (endian, fields) = self.parse_binary_layout_parts()?;
        Ok(Expr::BinaryLayout { endian, fields })
    }

    /// Parses a descriptor-backed binary layout pattern.
    pub(super) fn parse_binary_layout_pattern(&mut self) -> ParseResult<Pattern> {
        let (endian, fields) = self.parse_binary_layout_parts()?;
        Ok(Pattern::BinaryLayout { endian, fields })
    }

    /// Parses the shared `Binary[endian] { field: Descriptor }` payload.
    fn parse_binary_layout_parts(&mut self) -> ParseResult<(String, Vec<BinaryLayoutField>)> {
        let start = self.expect(TokenKind::Var)?.span();
        self.expect(TokenKind::LBracket)?;
        let endian = self.expect_lower_ident("expected binary endian policy `big` or `little`")?;
        if !matches!(endian.as_str(), "big" | "little") {
            return Err(ParseError {
                message: "binary layout endian must be `big` or `little`".to_string(),
                span: self.previous().span(),
            });
        }
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        if !self.consume_if(TokenKind::RBrace) {
            loop {
                fields.push(self.parse_binary_layout_field()?);
                if !self.consume_if(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace)?;
        }

        validate_binary_layout_fields(&fields, start)?;
        Ok((endian, fields))
    }

    /// Parses one named binary layout field and its descriptor type.
    fn parse_binary_layout_field(&mut self) -> ParseResult<BinaryLayoutField> {
        let name = self.expect_lower_ident("expected lower-case binary layout field name")?;
        self.expect(TokenKind::Colon)?;
        let descriptor = self.parse_type_expr(&[TokenKind::Comma, TokenKind::RBrace])?;
        validate_binary_descriptor_type(&descriptor)?;
        Ok(BinaryLayoutField { name, descriptor })
    }
}

/// Validates field-level binary layout invariants that are syntax-visible.
fn validate_binary_layout_fields(fields: &[BinaryLayoutField], span: Span) -> ParseResult<()> {
    let mut names = BTreeSet::new();
    let mut rest_count = 0;
    for field in fields {
        if !names.insert(field.name.as_str()) {
            return Err(ParseError {
                message: format!("duplicate binary layout field `{}`", field.name),
                span: field.descriptor.span,
            });
        }
        if descriptor_is_rest(&field.descriptor.text) {
            rest_count += 1;
            if rest_count > 1 {
                return Err(ParseError {
                    message: "binary layouts allow only one Rest field".to_string(),
                    span: field.descriptor.span,
                });
            }
        }
    }
    for (index, field) in fields.iter().enumerate() {
        if descriptor_is_rest(&field.descriptor.text) && index + 1 != fields.len() {
            return Err(ParseError {
                message: "binary layout Rest field must be terminal".to_string(),
                span: field.descriptor.span,
            });
        }
    }
    if fields.is_empty() {
        return Err(ParseError {
            message: "binary layouts require at least one descriptor field".to_string(),
            span,
        });
    }
    Ok(())
}

/// Validates the descriptor type accepted in a binary layout field.
fn validate_binary_descriptor_type(descriptor: &TypeExpr) -> ParseResult<()> {
    let text = descriptor.text.trim();
    let valid = descriptor_is_rest(text)
        || descriptor_is_utf_scalar(text)
        || descriptor_has_width(text, "UInt")
        || descriptor_has_width(text, "IntBits")
        || descriptor_has_width(text, "Bytes")
        || descriptor_has_width(text, "Bits");
    if valid {
        return Ok(());
    }
    Err(ParseError {
        message: format!("binary layout field uses unsupported descriptor `{text}`"),
        span: descriptor.span,
    })
}

/// Returns whether `text` is a canonical Unicode scalar descriptor.
fn descriptor_is_utf_scalar(text: &str) -> bool {
    let text = text.trim();
    ["Utf8", "Utf16", "Utf32"]
        .into_iter()
        .any(|name| matches_descriptor_name(text, name))
}

/// Returns whether `text` is the terminal rest descriptor.
fn descriptor_is_rest(text: &str) -> bool {
    matches!(
        text.trim(),
        "Rest" | "std.binary.Binary.Rest" | "std.binary.Rest"
    )
}

/// Returns whether `text` is a canonical width-bearing descriptor.
fn descriptor_has_width(text: &str, name: &str) -> bool {
    let text = text.trim();
    let Some(open) = text.find('[') else {
        return false;
    };
    let Some(close) = text.strip_suffix(']') else {
        return false;
    };
    let head = text[..open].trim();
    if !matches_descriptor_name(head, name) {
        return false;
    }
    let width = close[open + 1..].trim();
    !width.is_empty() && width.chars().all(|ch| ch.is_ascii_digit()) && width != "0"
}

/// Returns whether a descriptor head resolves to the expected canonical name.
fn matches_descriptor_name(head: &str, name: &str) -> bool {
    head == name
        || head
            .strip_prefix("std.binary.Binary.")
            .is_some_and(|suffix| suffix == name)
        || head
            .strip_prefix("std.binary.")
            .is_some_and(|suffix| suffix == name)
}
