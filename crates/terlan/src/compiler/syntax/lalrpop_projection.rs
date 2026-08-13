//! Contextual token projections needed to keep the canonical grammar LR(1).

use super::token::{Token, TokenKind};

/// Returns the closing `>` for a constraint list that follows square-bracket
/// generic parameters and immediately precedes a callable parameter list.
pub(super) fn head_constraint_list_end(tokens: &[Token], index: usize) -> Option<usize> {
    if tokens.get(index)?.kind != TokenKind::Lt {
        return None;
    }
    let previous = tokens[..index]
        .iter()
        .rev()
        .find(|token| !is_trivia(&token.kind))?;
    if previous.kind != TokenKind::RBracket {
        return None;
    }
    let mut angle_depth = 0usize;
    let mut delimiter_depth = 0usize;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match token.kind {
            TokenKind::Lt if delimiter_depth == 0 => angle_depth += 1,
            TokenKind::Gt if delimiter_depth == 0 => {
                angle_depth = angle_depth.checked_sub(1)?;
                if angle_depth == 0 {
                    let close = index + offset;
                    let next = tokens[close + 1..]
                        .iter()
                        .find(|token| !is_trivia(&token.kind));
                    return next
                        .is_some_and(|token| token.kind == TokenKind::LParen)
                        .then_some(close);
                }
            }
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                delimiter_depth += 1;
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                delimiter_depth = delimiter_depth.saturating_sub(1);
            }
            TokenKind::EOF => return None,
            _ => {}
        }
    }
    None
}

/// Returns whether this parenthesis opens a top-level function clause head.
pub(super) fn is_function_clause_open(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::LParen)
        || tokens[..index]
            .iter()
            .fold(0isize, |depth, token| match token.kind {
                TokenKind::LBrace => depth + 1,
                TokenKind::RBrace => depth - 1,
                _ => depth,
            })
            != 0
    {
        return false;
    }
    let mut preceding = tokens[..index]
        .iter()
        .rev()
        .filter(|token| !is_trivia(&token.kind));
    let previous = preceding.next();
    let before_name = preceding.next();
    if before_name.is_some_and(|token| {
        !matches!(
            token.kind,
            TokenKind::Dot | TokenKind::Semicolon | TokenKind::Pub
        )
    }) {
        return false;
    }
    if previous.is_none_or(|token| !matches!(token.kind, TokenKind::Atom | TokenKind::Var)) {
        return false;
    }
    if before_name.is_some_and(|token| {
        token.kind == TokenKind::Dot && previous.is_some_and(|name| token.end == name.start)
    }) {
        return false;
    }
    let mut depth = 0usize;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match &token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth -= 1;
                if depth == 0 {
                    return tokens[index + offset + 1..]
                        .iter()
                        .find(|token| !is_trivia(&token.kind))
                        .is_some_and(|token| {
                            token.kind == TokenKind::Arrow || token.kind == TokenKind::Where
                        });
                }
            }
            TokenKind::EOF => return false,
            _ => {}
        }
    }
    false
}

/// Finds declaration arrows that separate a return type from its body.
///
/// Function types and lambda expressions deliberately share `(...) -> ...`
/// syntax. Classifying the declaration separator first prevents a parenthesized
/// function return type from swallowing the function body as another arrow
/// return type.
pub(super) fn function_body_arrow_indices(tokens: &[Token]) -> Vec<usize> {
    let mut arrows = Vec::new();
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut signature_depths = Vec::<(usize, usize, usize)>::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen => parens += 1,
            TokenKind::RParen => parens = parens.saturating_sub(1),
            TokenKind::LBracket => brackets += 1,
            TokenKind::RBracket => brackets = brackets.saturating_sub(1),
            TokenKind::LBrace => braces += 1,
            TokenKind::RBrace => {
                braces = braces.saturating_sub(1);
                signature_depths.retain(|depth| depth.2 <= braces);
            }
            TokenKind::Colon if parens == 0 && brackets == 0 => {
                if colon_follows_callable_head(tokens, index) {
                    signature_depths.push((parens, brackets, braces));
                }
            }
            TokenKind::Arrow
                if signature_depths
                    .last()
                    .is_some_and(|depth| *depth == (parens, brackets, braces))
                    && arrow_starts_expression(tokens, index) =>
            {
                arrows.push(index);
                signature_depths.pop();
            }
            TokenKind::Dot if parens == 0 && brackets == 0 => {
                signature_depths.retain(|depth| depth.2 < braces);
            }
            TokenKind::EOF => break,
            _ => {}
        }
    }
    arrows
}

/// Finds parenthesis tokens that introduce lambda syntax.
pub(super) fn lambda_opening_indices(
    tokens: &[Token],
    function_body_arrows: &[usize],
) -> Vec<usize> {
    let mut stack = Vec::new();
    let mut openings = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen => stack.push(index),
            TokenKind::RParen => {
                let Some(opening) = stack.pop() else {
                    continue;
                };
                let next = tokens[index + 1..]
                    .iter()
                    .find(|token| !is_trivia(&token.kind));
                let previous = tokens[..opening]
                    .iter()
                    .rev()
                    .find(|token| !is_trivia(&token.kind));
                let clause_group = previous.is_some_and(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Where | TokenKind::When | TokenKind::And | TokenKind::Or
                    )
                }) || inside_if_clause_guard(tokens, opening);
                let next_index = tokens[index + 1..]
                    .iter()
                    .position(|token| !is_trivia(&token.kind))
                    .map(|offset| index + 1 + offset);
                if next.is_some_and(|token| token.kind == TokenKind::Arrow)
                    && next_index
                        .is_none_or(|arrow| function_body_arrows.binary_search(&arrow).is_err())
                    && previous.is_none_or(|token| !can_end_expression(&token.kind))
                    && !clause_group
                {
                    openings.push(opening);
                }
            }
            _ => {}
        }
    }
    openings.sort_unstable();
    openings
}

fn colon_follows_callable_head(tokens: &[Token], colon: usize) -> bool {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    for token in tokens[..colon].iter().rev() {
        match token.kind {
            TokenKind::RParen if parens == 0 && brackets == 0 && braces == 0 => return true,
            TokenKind::RParen => parens += 1,
            TokenKind::LParen if parens > 0 => parens -= 1,
            TokenKind::RBracket => brackets += 1,
            TokenKind::LBracket if brackets > 0 => brackets -= 1,
            TokenKind::RBrace => braces += 1,
            TokenKind::LBrace if braces > 0 => braces -= 1,
            TokenKind::Dot | TokenKind::Semicolon
                if parens == 0 && brackets == 0 && braces == 0 =>
            {
                return false;
            }
            _ => {}
        }
    }
    false
}

fn arrow_starts_expression(tokens: &[Token], arrow: usize) -> bool {
    let Some(next) = tokens[arrow + 1..]
        .iter()
        .find(|token| !is_trivia(&token.kind))
    else {
        return false;
    };
    match next.kind {
        TokenKind::Atom => true,
        TokenKind::Var => next.text.chars().next().is_some_and(char::is_lowercase),
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::String
        | TokenKind::Let
        | TokenKind::If
        | TokenKind::Case
        | TokenKind::Try
        | TokenKind::LParen
        | TokenKind::LBracket
        | TokenKind::LBrace
        | TokenKind::Minus
        | TokenKind::Bang => true,
        _ => false,
    }
}

/// Finds top-level lambda-body separators that are not owned by a leading `let`.
pub(super) fn lambda_sequence_semicolon_indices(
    tokens: &[Token],
    openings: &[usize],
) -> Vec<usize> {
    let mut separators = Vec::new();
    for &opening in openings {
        let Some(body_start) = lambda_body_start(tokens, opening) else {
            continue;
        };
        let body_starts_with_let = tokens[body_start..]
            .iter()
            .find(|token| !is_trivia(&token.kind))
            .is_some_and(|token| token.kind == TokenKind::Let);
        let mut leave_let_separator = body_starts_with_let;
        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        for (offset, token) in tokens[body_start..].iter().enumerate() {
            let index = body_start + offset;
            match token.kind {
                TokenKind::LParen => parens += 1,
                TokenKind::LBracket => brackets += 1,
                TokenKind::LBrace => braces += 1,
                TokenKind::RParen if parens == 0 && brackets == 0 && braces == 0 => break,
                TokenKind::RBracket if parens == 0 && brackets == 0 && braces == 0 => break,
                TokenKind::RBrace if parens == 0 && brackets == 0 && braces == 0 => break,
                TokenKind::RParen => parens -= 1,
                TokenKind::RBracket => brackets -= 1,
                TokenKind::RBrace => braces -= 1,
                TokenKind::Comma if parens == 0 && brackets == 0 && braces == 0 => break,
                TokenKind::Dot if parens == 0 && brackets == 0 && braces == 0 => {
                    let next = tokens[index + 1..]
                        .iter()
                        .find(|next| !is_trivia(&next.kind));
                    if next.is_none_or(|next| token.end != next.start) {
                        break;
                    }
                }
                TokenKind::Semicolon if parens == 0 && brackets == 0 && braces == 0 => {
                    if leave_let_separator {
                        leave_let_separator = tokens[index + 1..]
                            .iter()
                            .find(|next| !is_trivia(&next.kind))
                            .is_some_and(|next| next.kind == TokenKind::Let);
                    } else {
                        separators.push(index);
                    }
                }
                TokenKind::EOF => break,
                _ => {}
            }
        }
    }
    separators.sort_unstable();
    separators.dedup();
    separators
}

/// Finds semicolons that separate clauses at the owning `if` block depth.
pub(super) fn if_clause_semicolon_indices(tokens: &[Token]) -> Vec<usize> {
    let mut separators = Vec::new();
    for (opening, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::LBrace
            || tokens[..opening]
                .iter()
                .rev()
                .find(|previous| !is_trivia(&previous.kind))
                .is_none_or(|previous| previous.kind != TokenKind::If)
        {
            continue;
        }
        let Some(close) = balanced_brace_close(tokens, opening) else {
            continue;
        };
        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        for (offset, candidate) in tokens[opening + 1..close].iter().enumerate() {
            let index = opening + 1 + offset;
            match candidate.kind {
                TokenKind::LParen => parens += 1,
                TokenKind::RParen => parens -= 1,
                TokenKind::LBracket => brackets += 1,
                TokenKind::RBracket => brackets -= 1,
                TokenKind::LBrace => braces += 1,
                TokenKind::RBrace => braces -= 1,
                TokenKind::Semicolon
                    if parens == 0
                        && brackets == 0
                        && braces == 0
                        && following_if_segment_has_arrow(tokens, index + 1, close) =>
                {
                    separators.push(index);
                }
                _ => {}
            }
        }
    }
    separators.sort_unstable();
    separators.dedup();
    separators
}

fn balanced_brace_close(tokens: &[Token], opening: usize) -> Option<usize> {
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

fn following_if_segment_has_arrow(tokens: &[Token], start: usize, close: usize) -> bool {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    for token in &tokens[start..close] {
        match token.kind {
            TokenKind::LParen => parens += 1,
            TokenKind::RParen => parens -= 1,
            TokenKind::LBracket => brackets += 1,
            TokenKind::RBracket => brackets -= 1,
            TokenKind::LBrace => braces += 1,
            TokenKind::RBrace => braces -= 1,
            TokenKind::Arrow if parens == 0 && brackets == 0 && braces == 0 => return true,
            TokenKind::Semicolon if parens == 0 && brackets == 0 && braces == 0 => return false,
            _ => {}
        }
    }
    false
}

fn lambda_body_start(tokens: &[Token], opening: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, token) in tokens[opening..].iter().enumerate() {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth -= 1;
                if depth == 0 {
                    let close = opening + offset;
                    let (arrow_offset, arrow) = tokens[close + 1..]
                        .iter()
                        .enumerate()
                        .find(|(_, token)| !is_trivia(&token.kind))?;
                    return (arrow.kind == TokenKind::Arrow)
                        .then_some(close + 1 + arrow_offset + 1);
                }
            }
            TokenKind::EOF => return None,
            _ => {}
        }
    }
    None
}

fn inside_if_clause_guard(tokens: &[Token], index: usize) -> bool {
    let mut stack = Vec::new();
    for (token_index, token) in tokens[..index].iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => stack.push(token_index),
            TokenKind::RBrace => {
                stack.pop();
            }
            _ => {}
        }
    }
    let Some(opening) = stack.last().copied() else {
        return false;
    };
    tokens[..opening]
        .iter()
        .rev()
        .find(|token| !is_trivia(&token.kind))
        .is_some_and(|token| token.kind == TokenKind::If)
        && !if_clause_prefix_has_arrow(&tokens[opening + 1..index])
}

fn if_clause_prefix_has_arrow(tokens: &[Token]) -> bool {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut has_arrow = false;
    for token in tokens {
        match token.kind {
            TokenKind::LParen => parens += 1,
            TokenKind::RParen => parens = parens.saturating_sub(1),
            TokenKind::LBracket => brackets += 1,
            TokenKind::RBracket => brackets = brackets.saturating_sub(1),
            TokenKind::LBrace => braces += 1,
            TokenKind::RBrace => braces = braces.saturating_sub(1),
            TokenKind::Arrow if parens == 0 && brackets == 0 && braces == 0 => has_arrow = true,
            TokenKind::Semicolon if parens == 0 && brackets == 0 && braces == 0 => {
                has_arrow = false;
            }
            _ => {}
        }
    }
    has_arrow
}

/// Returns whether this bracket contains type arguments for a following call.
pub(super) fn is_generic_call_open(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::LBracket)
    {
        return false;
    }
    let mut depth = 0usize;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match token.kind {
            TokenKind::LBracket => depth += 1,
            TokenKind::RBracket => {
                depth -= 1;
                if depth == 0 {
                    let Some((call_offset, _)) = tokens[index + offset + 1..]
                        .iter()
                        .enumerate()
                        .find(|(_, token)| !is_trivia(&token.kind))
                    else {
                        return false;
                    };
                    let call_open = index + offset + 1 + call_offset;
                    return tokens[call_open].kind == TokenKind::LParen
                        && !call_is_followed_by_signature(tokens, call_open);
                }
            }
            TokenKind::EOF => return false,
            _ => {}
        }
    }
    false
}

/// Reports whether a colon separates the module and function of a remote call.
pub(super) fn is_remote_call_colon(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index)
        .is_none_or(|token| token.kind != TokenKind::Colon)
    {
        return false;
    }
    let previous = tokens[..index]
        .iter()
        .rev()
        .find(|token| !is_trivia(&token.kind));
    let mut following = tokens[index + 1..]
        .iter()
        .enumerate()
        .filter(|(_, token)| !is_trivia(&token.kind));
    let Some((_function_offset, function)) = following.next() else {
        return false;
    };
    if previous.is_none_or(|token| !matches!(token.kind, TokenKind::Atom | TokenKind::Var))
        || !matches!(function.kind, TokenKind::Atom | TokenKind::Var)
        || previous.is_none_or(|token| token.end != tokens[index].start)
        || tokens[index].end != function.start
    {
        return false;
    }
    let Some((next_offset, next)) = following.next() else {
        return false;
    };
    next.kind == TokenKind::LParen
        || (next.kind == TokenKind::LBracket
            && is_generic_call_open(tokens, index + 1 + next_offset))
}

fn call_is_followed_by_signature(tokens: &[Token], opening: usize) -> bool {
    let mut depth = 0usize;
    for (offset, token) in tokens[opening..].iter().enumerate() {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth -= 1;
                if depth == 0 {
                    return tokens[opening + offset + 1..]
                        .iter()
                        .find(|token| !is_trivia(&token.kind))
                        .is_some_and(|token| {
                            matches!(token.kind, TokenKind::Colon | TokenKind::LBracket)
                        });
                }
            }
            TokenKind::EOF => return false,
            _ => {}
        }
    }
    false
}

/// Finds the balanced close of an `html { ... }` raw block.
pub(super) fn html_raw_block_end(tokens: &[Token], index: usize) -> Option<usize> {
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
pub(super) fn expression_raw_macro_end(tokens: &[Token], index: usize) -> Option<usize> {
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
pub(super) fn native_raw_declaration_end(tokens: &[Token], index: usize) -> Option<usize> {
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
pub(super) fn config_raw_declaration_end(tokens: &[Token], index: usize) -> Option<usize> {
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
pub(super) fn is_binary_layout_open(tokens: &[Token], index: usize) -> bool {
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
pub(super) fn is_nominal_keyed_open(tokens: &[Token], index: usize) -> bool {
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

pub(super) fn is_trivia(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comment
            | TokenKind::DocBlockComment
            | TokenKind::DocComment
            | TokenKind::ModuleDocComment
    )
}

fn can_end_expression(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Atom
            | TokenKind::Var
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::String
            | TokenKind::Binary
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
    )
}
