use super::*;

const MAX_SYNTACTIC_NESTING: usize = 16;

/// Rejects token streams that exceed the recursive parser's safe depth.
///
/// Inputs:
/// - `tokens`: lexer output with string and binary payloads already isolated.
///
/// Output:
/// - `Ok(())` when delimiter nesting stays within the parser budget.
/// - Parser diagnostic anchored at the token that exceeds the budget.
///
/// Transformation:
/// - Counts only syntactic delimiter tokens so literal payload text does not
///   influence parser depth checks.
pub(super) fn ensure_token_nesting_within_limit(tokens: &[Token]) -> ParseResult<()> {
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                if depth > MAX_SYNTACTIC_NESTING {
                    return Err(ParseError {
                        message: format!(
                            "source nesting depth exceeds maximum of {MAX_SYNTACTIC_NESTING}"
                        ),
                        span: token.span(),
                    });
                }
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    Ok(())
}
