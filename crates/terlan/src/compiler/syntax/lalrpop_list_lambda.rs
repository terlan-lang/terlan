//! Contextual list-lambda ranges used by the generated-parser boundary.

use super::{
    lalrpop_projection::is_trivia,
    token::{Token, TokenKind},
};

/// Finds lambdas used directly as list elements and the delimiter ending each
/// element. The list grammar reserves a top-level `|` for cons/comprehension
/// syntax, so it cannot embed the full recursive assignment grammar without a
/// large LR state expansion. The boundary uses these ranges to retry only the
/// rejected value lambda through the ordinary grouped-expression production.
pub(super) fn direct_list_lambda_groups(
    tokens: &[Token],
    lambda_openings: &[usize],
) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    for &opening in lambda_openings {
        let mut delimiters = Vec::new();
        for (index, token) in tokens[..opening].iter().enumerate() {
            match token.kind {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    delimiters.push((index, token.kind.clone()));
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    delimiters.pop();
                }
                _ => {}
            }
        }
        let Some(&(list_opening, TokenKind::LBracket)) = delimiters.last() else {
            continue;
        };
        let previous = tokens[..opening]
            .iter()
            .enumerate()
            .rev()
            .find(|(_, token)| !is_trivia(&token.kind));
        let starts_element = previous.is_some_and(|(index, token)| {
            index == list_opening
                || (token.kind == TokenKind::Comma
                    && delimiter_owner(tokens, index) == Some(list_opening))
        });
        if !starts_element {
            continue;
        }

        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        for (index, token) in tokens.iter().enumerate().skip(opening) {
            match token.kind {
                TokenKind::LParen => parens += 1,
                TokenKind::RParen => parens = parens.saturating_sub(1),
                TokenKind::LBracket => brackets += 1,
                TokenKind::RBracket if parens == 0 && brackets == 0 && braces == 0 => {
                    groups.push((opening, index));
                    break;
                }
                TokenKind::RBracket => brackets = brackets.saturating_sub(1),
                TokenKind::LBrace => braces += 1,
                TokenKind::RBrace => braces = braces.saturating_sub(1),
                TokenKind::Comma | TokenKind::Pipe
                    if parens == 0 && brackets == 0 && braces == 0 =>
                {
                    groups.push((opening, index));
                    break;
                }
                TokenKind::EOF => break,
                _ => {}
            }
        }
    }
    groups
}

fn delimiter_owner(tokens: &[Token], target: usize) -> Option<usize> {
    let mut stack = Vec::new();
    for (index, token) in tokens[..target].iter().enumerate() {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => stack.push(index),
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack.last().copied()
}

#[cfg(test)]
#[path = "lalrpop_list_lambda_test.rs"]
mod tests;
