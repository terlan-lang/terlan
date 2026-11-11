//! Structural and raw-block token projections.

use super::{balanced_brace_close, is_trivia};
use crate::compiler::syntax::token::{Token, TokenKind};

/// Finds the balanced close of an `html { ... }` raw block.
pub(in crate::compiler::syntax) fn html_raw_block_end(
    tokens: &[Token],
    index: usize,
) -> Option<usize> {
    if tokens.get(index)?.kind != TokenKind::LBrace
        || tokens[..index]
            .iter()
            .rev()
            .find(|token| !is_trivia(&token.kind))
            .is_none_or(|token| token.text != "html")
    {
        return None;
    }
    let mut depth = 0usize;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Finds a complete expression-level raw macro starting at its name.
///
/// Untyped custom macros require an adjacent opening brace. Built-in HTML and
/// typed SQL retain their canonical whitespace rules. Collapsing the balanced
/// payload keeps protocol-specific raw text out of the LR grammar while the
/// resulting node still retains the exact source span for explicit lowering.
pub(in crate::compiler::syntax) fn expression_raw_macro_end(
    tokens: &[Token],
    index: usize,
) -> Option<usize> {
    let name = tokens.get(index)?;
    if name.kind != TokenKind::Atom {
        return None;
    }
    let significant = tokens[index + 1..]
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia(&token.kind))
        .collect::<Vec<_>>();
    let opening = if name.text == "sql"
        && significant
            .first()
            .is_some_and(|(_, token)| token.kind == TokenKind::LBracket)
    {
        let mut bracket_depth = 0usize;
        let mut close_offset = None;
        for (offset, token) in &significant {
            match token.kind {
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    if bracket_depth == 0 {
                        close_offset = Some(*offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close_offset = close_offset?;
        let (brace_offset, brace) = significant
            .iter()
            .find(|(offset, _)| *offset > close_offset)?;
        (brace.kind == TokenKind::LBrace).then_some(index + 1 + *brace_offset)?
    } else {
        let (offset, brace) = significant.first()?;
        if brace.kind != TokenKind::LBrace {
            return None;
        }
        let adjacent = name.end == brace.start;
        if !adjacent && name.text != "html" {
            return None;
        }
        index + 1 + *offset
    };

    balanced_brace_close(tokens, opening)
}

/// Finds the balanced end of a retired `native core module` declaration.
pub(in crate::compiler::syntax) fn native_raw_declaration_end(
    tokens: &[Token],
    index: usize,
) -> Option<usize> {
    if tokens.get(index)?.text != "native" {
        return None;
    }
    let significant = tokens[index..]
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia(&token.kind))
        .collect::<Vec<_>>();
    if significant.get(1)?.1.text != "core"
        || significant.get(2)?.1.kind != TokenKind::Module
        || significant
            .get(3)
            .is_none_or(|(_, token)| token.kind != TokenKind::Var)
        || significant.get(4)?.1.kind != TokenKind::LBrace
    {
        return None;
    }
    let opening = index + significant[4].0;
    let mut depth = 0usize;
    for (offset, token) in tokens[opening..].iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => {
                depth -= 1;
                if depth == 0 {
                    return Some(opening + offset);
                }
            }
            TokenKind::EOF => return None,
            _ => {}
        }
    }
    None
}

/// Finds the balanced body of `target name { ... }` configuration syntax.
pub(in crate::compiler::syntax) fn config_raw_declaration_end(
    tokens: &[Token],
    index: usize,
) -> Option<usize> {
    if tokens.get(index)?.text != "target" {
        return None;
    }
    let significant = tokens[index + 1..]
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia(&token.kind))
        .collect::<Vec<_>>();
    if significant
        .first()
        .is_none_or(|(_, token)| !matches!(token.kind, TokenKind::Atom | TokenKind::Var))
    {
        return None;
    }
    let (opening_offset, opening) = significant.get(1)?;
    if opening.kind != TokenKind::LBrace {
        return None;
    }
    balanced_brace_close(tokens, index + 1 + *opening_offset)
}

/// Returns whether this bracket opens a descriptor-backed binary layout.
pub(in crate::compiler::syntax) fn is_binary_layout_open(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::LBracket)
    {
        return false;
    }
    let previous = tokens[..index]
        .iter()
        .rev()
        .find(|token| !is_trivia(&token.kind));
    if previous.is_none_or(|token| token.kind != TokenKind::Var || token.text != "Binary") {
        return false;
    }
    let mut following = tokens[index + 1..]
        .iter()
        .filter(|token| !is_trivia(&token.kind));
    following
        .next()
        .is_some_and(|token| token.kind == TokenKind::Atom)
        && following
            .next()
            .is_some_and(|token| token.kind == TokenKind::RBracket)
        && following
            .next()
            .is_some_and(|token| token.kind == TokenKind::LBrace)
}

/// Returns whether this brace starts a nominal keyed value or pattern.
///
/// Terlan permits both `case Value { ... }` and `Value { field: value }`.
/// The canonical lexer intentionally does not distinguish those braces. The
/// colon immediately inside a brace following a type name is the stable local
/// discriminator; semantic pattern/expression classification remains a later
/// syntax-output normalization step.
pub(in crate::compiler::syntax) fn is_nominal_keyed_open(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::LBrace)
    {
        return false;
    }
    let mut preceding = tokens[..index]
        .iter()
        .rev()
        .filter(|token| !is_trivia(&token.kind));
    let previous = preceding.next();
    if brace_opens_declaration_body(tokens, index)
        || previous.is_none_or(|token| token.kind != TokenKind::Var)
        || preceding.next().is_some_and(is_declaration_introducer)
    {
        return false;
    }
    let mut following = tokens[index + 1..]
        .iter()
        .filter(|token| !is_trivia(&token.kind));
    let Some(mut first) = following.next() else {
        return false;
    };
    if first.kind == TokenKind::Hash {
        let Some(field) = following.next() else {
            return false;
        };
        first = field;
    }
    if first.kind == TokenKind::RBrace {
        return !empty_brace_opens_impl_body(tokens, index);
    }
    matches!(first.kind, TokenKind::Atom | TokenKind::String)
        && following
            .next()
            .is_some_and(|token| token.kind == TokenKind::Colon)
}

fn brace_opens_declaration_body(tokens: &[Token], index: usize) -> bool {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    for token in tokens[..index].iter().rev() {
        match token.kind {
            TokenKind::RParen => parens += 1,
            TokenKind::LParen => parens = parens.saturating_sub(1),
            TokenKind::RBracket => brackets += 1,
            TokenKind::LBracket => brackets = brackets.saturating_sub(1),
            TokenKind::Dot if parens == 0 && brackets == 0 => return false,
            TokenKind::Struct | TokenKind::Trait | TokenKind::Constructor | TokenKind::Impl
                if parens == 0 && brackets == 0 =>
            {
                return true;
            }
            TokenKind::Arrow | TokenKind::Equals | TokenKind::Semicolon | TokenKind::RBrace
                if parens == 0 && brackets == 0 =>
            {
                return false;
            }
            _ => {}
        }
    }
    false
}

fn empty_brace_opens_impl_body(tokens: &[Token], index: usize) -> bool {
    let mut saw_for = false;
    for token in tokens[..index]
        .iter()
        .rev()
        .filter(|token| !is_trivia(&token.kind))
    {
        match token.kind {
            TokenKind::For => saw_for = true,
            TokenKind::Impl => return saw_for,
            TokenKind::Arrow
            | TokenKind::Equals
            | TokenKind::Semicolon
            | TokenKind::LBrace
            | TokenKind::RBrace => return false,
            _ => {}
        }
    }
    false
}

fn is_declaration_introducer(token: &Token) -> bool {
    matches!(
        token.kind,
        TokenKind::Struct
            | TokenKind::Constructor
            | TokenKind::Trait
            | TokenKind::Template
            | TokenKind::Impl
            | TokenKind::Implements
            | TokenKind::Includes
            | TokenKind::Extends
            | TokenKind::For
    ) || matches!(token.text.as_str(), "shape" | "config" | "annotation")
}
