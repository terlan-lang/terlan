use super::*;
use crate::terlan_syntax::{
    AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC, NEGATIVE_STRUCTURAL_IMPLICATION_DIAGNOSTIC,
};
use std::collections::HashSet;

impl Parser {
    /// Parses one type parameter, including optional structural evidence.
    pub(super) fn parse_type_param_text(&mut self) -> ParseResult<String> {
        let start = self.current().start;
        let mut text = String::new();

        if self.consume_if(TokenKind::Const) {
            let name = self.current().clone();
            if name.kind != TokenKind::Var || !super::constants::is_screaming_snake_case(&name.text)
            {
                return Err(ParseError {
                    message: "const generic parameter names must use SCREAMING_SNAKE_CASE"
                        .to_string(),
                    span: name.span(),
                });
            }
            self.bump();
            self.expect(TokenKind::Colon)?;
            let kind = self.current().clone();
            if kind.kind != TokenKind::Var || !matches!(kind.text.as_str(), "Int" | "Bool" | "Atom")
            {
                return Err(ParseError {
                    message: "const generic kind must be Int, Bool, or Atom".to_string(),
                    span: kind.span(),
                });
            }
            self.bump();
            return Ok(format!("const {}: {}", name.text, kind.text));
        }

        if self.check(TokenKind::Plus) || self.check(TokenKind::Minus) {
            text.push_str(&self.bump().text);
        }

        let name = self.current().clone();
        if name.kind != TokenKind::Var {
            return Err(ParseError {
                message: "expected upper-case type parameter name".to_string(),
                span: name.span(),
            });
        }
        self.bump();
        text.push_str(&name.text);

        if self.consume_if(TokenKind::LBracket) {
            text.push('[');
            self.parse_higher_kind_slots(&mut text, start)?;
            self.expect(TokenKind::RBracket)?;
            text.push(']');
        }

        if self.consume_if(TokenKind::FatArrow) {
            text.push_str(" => ");
            text.push_str(&self.parse_structural_implication_shape()?);
        }

        Ok(text)
    }

    /// Parses a closed positive structural implication target.
    fn parse_structural_implication_shape(&mut self) -> ParseResult<String> {
        if self.check_keyword("not") {
            return Err(ParseError {
                message: NEGATIVE_STRUCTURAL_IMPLICATION_DIAGNOSTIC.to_string(),
                span: self.current().span(),
            });
        }
        if !self.consume_if(TokenKind::LBrace) {
            return Err(ParseError {
                message: "implication target must be a closed structural field shape".to_string(),
                span: self.current().span(),
            });
        }
        if self.check(TokenKind::RBrace) {
            return Err(ParseError {
                message: "implication target must contain at least one field".to_string(),
                span: self.current().span(),
            });
        }

        let mut fields = Vec::new();
        let mut field_names = HashSet::new();
        loop {
            let field = self.current().clone();
            if field.kind != TokenKind::Atom {
                return Err(ParseError {
                    message: "expected lower-case implication field name".to_string(),
                    span: field.span(),
                });
            }
            if !field_names.insert(field.text.clone()) {
                return Err(ParseError {
                    message: AMBIGUOUS_STRUCTURAL_IMPLICATION_DIAGNOSTIC.to_string(),
                    span: field.span(),
                });
            }
            self.bump();
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_implication_type_expr(&[TokenKind::Comma, TokenKind::RBrace])?;
            fields.push(format!("{}: {}", field.text, ty.text));
            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(format!("{{{}}}", fields.join(", ")))
    }

    /// Parses `_` slots for a higher-kinded type parameter.
    fn parse_higher_kind_slots(&mut self, text: &mut String, start: usize) -> ParseResult<()> {
        if self.check(TokenKind::RBracket) {
            return Err(ParseError {
                message: "higher-kinded type parameter requires at least one `_` slot".to_string(),
                span: Span::new(start, self.current().end),
            });
        }

        let mut first = true;
        loop {
            let variance = if self.check(TokenKind::Plus) || self.check(TokenKind::Minus) {
                let marker = self.current().text.clone();
                self.bump();
                Some(marker)
            } else {
                None
            };
            let slot = self.current().clone();
            if slot.text != "_" {
                return Err(ParseError {
                    message: "higher-kinded type parameter slots must be `_`, `+_`, or `-_`"
                        .to_string(),
                    span: slot.span(),
                });
            }
            self.bump();
            if !first {
                text.push_str(", ");
            }
            if let Some(marker) = variance {
                text.push_str(&marker);
            }
            text.push('_');
            first = false;

            if !self.consume_if(TokenKind::Comma) {
                break;
            }
        }

        Ok(())
    }
}
