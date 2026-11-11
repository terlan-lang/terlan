//! Contextual token projections needed to keep the canonical grammar LR(1).

use super::token::{Token, TokenKind};

mod structure;

pub(super) use structure::{
    config_raw_declaration_end, expression_raw_macro_end, html_raw_block_end,
    is_binary_layout_open, is_nominal_keyed_open, native_raw_declaration_end,
};

/// Finds each pattern immediately followed by `<-`, including the arrow.
/// Collapsing this prefix keeps bare refutable continuations LR(1); lowering
/// reparses the retained pattern source through the canonical pattern parser.
pub(super) fn refutable_pattern_ranges(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for (arrow, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::LtMinus {
            continue;
        }
        let Some(last) = (0..arrow)
            .rev()
            .find(|&index| !is_trivia(&tokens[index].kind))
        else {
            continue;
        };
        let mut parens = 0usize;
        let mut brackets = 0usize;
        let mut braces = 0usize;
        let mut boundary = None;
        for index in (0..=last).rev() {
            let kind = &tokens[index].kind;
            if is_trivia(kind) {
                continue;
            }
            match kind {
                TokenKind::RParen => parens += 1,
                TokenKind::RBracket => brackets += 1,
                TokenKind::RBrace => braces += 1,
                TokenKind::LParen if parens > 0 => parens -= 1,
                TokenKind::LBracket if brackets > 0 => brackets -= 1,
                TokenKind::LBrace if braces > 0 => braces -= 1,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace
                    if parens == 0 && brackets == 0 && braces == 0 =>
                {
                    boundary = Some(index);
                    break;
                }
                TokenKind::Semicolon
                | TokenKind::Pipe
                | TokenKind::Comma
                | TokenKind::Arrow
                | TokenKind::FatArrow
                | TokenKind::Equals
                    if parens == 0 && brackets == 0 && braces == 0 =>
                {
                    boundary = Some(index);
                    break;
                }
                _ => {}
            }
        }
        let search_start = boundary.map_or(0, |index| index + 1);
        if let Some(start) = (search_start..arrow).find(|&index| !is_trivia(&tokens[index].kind)) {
            ranges.push((start, arrow));
        }
    }
    ranges.sort_unstable();
    ranges.dedup();
    ranges
}

/// Returns the opening brace of `when { pattern <- value; ... } else { ... }`.
pub(super) fn refutable_when_open(tokens: &[Token], index: usize) -> Option<usize> {
    if tokens.get(index)?.kind != TokenKind::When {
        return None;
    }
    let opening =
        (index + 1..tokens.len()).find(|&candidate| !is_trivia(&tokens[candidate].kind))?;
    if tokens[opening].kind != TokenKind::LBrace {
        return None;
    }
    let mut depth = 0usize;
    let mut has_refutable = false;
    for candidate in opening..tokens.len() {
        match tokens[candidate].kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let next = (candidate + 1..tokens.len())
                        .find(|&next| !is_trivia(&tokens[next].kind))?;
                    return (has_refutable && tokens[next].text == "else").then_some(opening);
                }
            }
            TokenKind::LtMinus if depth == 1 => has_refutable = true,
            TokenKind::EOF => return None,
            _ => {}
        }
    }
    None
}

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
    let mut following = tokens[arrow + 1..]
        .iter()
        .filter(|token| !is_trivia(&token.kind))
        .peekable();
    let Some(mut next) = following.next() else {
        return false;
    };
    // A fully qualified return type starts with a lowercase namespace but
    // ends with its type name. Do not classify its arrow as a body separator.
    while matches!(next.kind, TokenKind::Atom | TokenKind::Var)
        && following
            .peek()
            .is_some_and(|dot| dot.kind == TokenKind::Dot && dot.start == next.end)
    {
        let mut qualified = following.clone();
        let dot = qualified.next().expect("peeked dot");
        let Some(part) = qualified.next().filter(|part| {
            part.start == dot.end && matches!(part.kind, TokenKind::Atom | TokenKind::Var)
        }) else {
            break;
        };
        next = part;
        following = qualified;
    }
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
    let refutable_starts = refutable_pattern_ranges(tokens)
        .into_iter()
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    let mut separators = Vec::new();
    for &opening in openings {
        let Some(body_start) = lambda_body_start(tokens, opening) else {
            continue;
        };
        let body_start_index = tokens[body_start..]
            .iter()
            .position(|token| !is_trivia(&token.kind))
            .map(|offset| body_start + offset);
        let mut leave_binding_separator = body_start_index.is_some_and(|index| {
            tokens[index].kind == TokenKind::Let
                || refutable_starts.binary_search(&index).is_ok()
                || refutable_when_open(tokens, index).is_some()
        });
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
                    if leave_binding_separator {
                        let next_index = tokens[index + 1..]
                            .iter()
                            .position(|next| !is_trivia(&next.kind))
                            .map(|offset| index + 1 + offset);
                        leave_binding_separator = next_index.is_some_and(|next| {
                            tokens[next].kind == TokenKind::Let
                                || refutable_starts.binary_search(&next).is_ok()
                                || refutable_when_open(tokens, next).is_some()
                        });
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
