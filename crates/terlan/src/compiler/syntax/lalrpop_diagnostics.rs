//! Stable diagnostics applied after a generated expression parse is rejected.

use std::collections::HashSet;

use super::{lalrpop_boundary::LalrpopBoundaryError, lexer::lex, span::Span, token::TokenKind};

pub(super) fn expression_preflight(source: &str) -> Option<LalrpopBoundaryError> {
    if source.trim() == "_" {
        let start = source.find('_').unwrap_or(0);
        return Some(diagnostic(
            "wildcard '_' is only valid in pattern position",
            start,
            start + 1,
        ));
    }
    ["=:=", "=/=", "/="]
        .into_iter()
        .find_map(|operator| source.find(operator).map(|start| (start, operator)))
        .map(|(start, operator)| {
            diagnostic(
                format!("deprecated equality operator `{operator}` is not valid Terlan syntax"),
                start,
                start + operator.chars().count(),
            )
        })
}

pub(super) fn expression_diagnostic(
    source: &str,
    fallback: LalrpopBoundaryError,
) -> LalrpopBoundaryError {
    if let Some(start) = source.find("=>") {
        return diagnostic(
            "the compile-time implication arrow `=>` is not a runtime expression operator",
            start,
            start + 2,
        );
    }
    if let Some((start, operator)) = ["=:=", "=/=", "/="]
        .into_iter()
        .find_map(|operator| source.find(operator).map(|start| (start, operator)))
    {
        return diagnostic(
            format!("deprecated equality operator `{operator}` is not valid Terlan syntax"),
            start,
            start + operator.chars().count(),
        );
    }
    if source.trim_start().starts_with('[') {
        if let Some(start) = source.find(" where ") {
            return diagnostic(
                "list comprehension filters use comma-separated boolean expressions",
                start + 1,
                start + 6,
            );
        }
    }
    fallback
}

/// Preserves declaration-sequence error spans at the malformed delimiter.
///
/// LALRPOP reports the first token it cannot shift. When that token follows a
/// semicolon directly, the malformed expression starts at the delimiter that
/// introduced it, which is also the span expected by compiler diagnostics.
pub(super) fn module_diagnostic(
    source: &str,
    mut fallback: LalrpopBoundaryError,
) -> LalrpopBoundaryError {
    if fallback.span.start > 0
        && source.is_char_boundary(fallback.span.start)
        && source[..fallback.span.start].ends_with(';')
    {
        fallback.span.start -= 1;
    }
    fallback
}

pub(super) fn module_preflight(source: &str) -> Option<LalrpopBoundaryError> {
    let trivia = trivia_spans(source);
    if let Some(start) = source.find("impl Contract[") {
        if source[start..].contains("=>") || source[start..].contains("->") {
            return Some(diagnostic(
                "Contract impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for now",
                start,
                start + "impl Contract".len(),
            ));
        }
    }
    if let Some(start) = source.find("=> not {") {
        return Some(diagnostic(
            "negative structural implications are not supported; use negative trait implementations for denied capabilities",
            start,
            start + 9,
        ));
    }
    if let Some(duplicate) = duplicate_field_in_braces(source) {
        let message = if source[..duplicate].contains("=>") {
            "ambiguous_implication: structural implication field names must be unique within each shape"
        } else {
            "duplicate_record_type_field: record type field names must be unique within each record"
        };
        return Some(diagnostic(message, 0, source.len()));
    }
    if let Some(start) = concrete_higher_kind_slot(source) {
        return Some(diagnostic(
            "higher-kinded type parameter slots must be `_`, `+_`, or `-_`",
            start,
            start + 1,
        ));
    }
    if let Some(start) = source.find("#{") {
        if source[..start].matches('"').count() % 2 == 1 {
            return Some(diagnostic(
                "string patterns use `${...}` captures; `#{...}` is not Terlan syntax",
                start,
                start + 2,
            ));
        }
    }
    if let Some(start) = source.match_indices("(#").find_map(|(start, _)| {
        source[start + 2..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
            .then_some(start)
    }) {
        return Some(diagnostic(
            "struct patterns must use Type { field: pattern } syntax",
            start + 1,
            start + 2,
        ));
    }
    if let Some(start) = source.find(".(") {
        return Some(diagnostic(
            "function-value dot-call syntax was removed; use `callee(args)`",
            start,
            start + 2,
        ));
    }
    if let Some(start) = unsupported_when_guard(source) {
        return Some(diagnostic(
            "Terlan clause guards use `where`; `when` is not supported",
            start,
            start + 4,
        ));
    }
    if let Some(error) = repeated_let_diagnostic(source) {
        return Some(error);
    }
    if let Some(start) = source
        .lines()
        .find(|line| line.trim_start().starts_with("export "))
        .and_then(|line| source.find(line))
    {
        return Some(diagnostic(
            "source export declarations are not part of canonical Terlan; use declaration-site `pub`",
            start,
            start + "export".len(),
        ));
    }
    let mut line_offset = 0usize;
    for raw_line in source.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let trimmed = line.trim();
        let start = line_offset + line.len() - line.trim_start().len();
        line_offset += raw_line.len();
        if trivia
            .iter()
            .any(|&(trivia_start, trivia_end)| start >= trivia_start && start < trivia_end)
        {
            continue;
        }
        if is_reverse_alias_head(trimmed) {
            return Some(diagnostic(
                "migration.function_head_pattern.invalid_alias_style: reverse alias function-head pattern syntax is rejected; use pattern-first aliasing `{pattern} = name: Type`; docs docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style",
                start,
                start + trimmed.len(),
            ));
        }
        if let Some(subject) = unsupported_annotation_subject(trimmed) {
            return Some(diagnostic(
                "annotation subjects are not supported in Terlan 0.0.1",
                start + subject,
                start + trimmed.len(),
            ));
        }
        if trimmed.starts_with("(Self:") {
            return Some(diagnostic(
                "expected lower-case method receiver name",
                start + 1,
                start + 5,
            ));
        }
        if let Some(receiver_type) = trimmed.strip_prefix("(self: ") {
            if receiver_type
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_lowercase())
            {
                return Some(diagnostic(
                    "expected upper-case type name",
                    start + 7,
                    start + 7 + receiver_type.split(')').next().unwrap_or_default().len(),
                ));
            }
        }
        if let Some(after_receiver) = trimmed
            .starts_with('(')
            .then(|| trimmed.split_once(") ").map(|(_, tail)| tail))
            .flatten()
        {
            if after_receiver
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_uppercase())
            {
                return Some(diagnostic(
                    "expected lower-case method name",
                    start,
                    start + trimmed.len(),
                ));
            }
        }
        if trimmed.starts_with("type ") || trimmed.starts_with("pub type ") {
            if trimmed
                .split_once('=')
                .is_some_and(|(_, body)| body.trim_start().starts_with("case "))
            {
                let token = trimmed.find("case").unwrap_or_default();
                return Some(diagnostic(
                    "runtime expression token 'case' is not valid in type position",
                    start + token,
                    start + token + 4,
                ));
            }
            if trimmed
                .split_once('=')
                .is_some_and(|(_, body)| body.contains("=>"))
            {
                return Some(diagnostic(
                    "implication constraints are only valid in generic parameter constraints",
                    start,
                    start + trimmed.len(),
                ));
            }
        }
        if trimmed.contains(':') && trimmed.contains("=>") && !trimmed.contains('[') {
            return Some(diagnostic(
                "implication constraints are not valid on struct fields; place them on the owning generic parameter list",
                start,
                start + trimmed.len(),
            ));
        }
        if let Some(arrow) = trimmed.find("=>") {
            let generic_close = trimmed.find(']');
            let runtime_body = trimmed.find("->").is_some_and(|body| arrow > body);
            if generic_close.is_none_or(|close| arrow > close) || runtime_body {
                return Some(diagnostic(
                    "the compile-time implication arrow `=>` is not a runtime expression operator",
                    start + arrow,
                    start + arrow + 2,
                ));
            }
        }
        if trimmed.starts_with("shape ")
            && trimmed
                .strip_prefix("shape ")
                .and_then(|tail| tail.chars().next())
                .is_some_and(|character| character.is_ascii_lowercase())
        {
            return Some(diagnostic(
                "shape synonym names must be upper-case",
                start,
                start + trimmed.len(),
            ));
        }
    }
    if let Some(start) = source.find(".*.") {
        return Some(diagnostic(
            "wildcard imports must use braced selector syntax",
            start,
            start + 2,
        ));
    }
    if let Some((start, end)) = significant_token_span(source, "derives") {
        return Some(diagnostic("expected LBrace", start, end));
    }
    if let Some(start) = source.find("impl Contract[") {
        if source[start..].contains("=>") || source[start..].contains("->") {
            return Some(diagnostic(
                "Contract impl syntax is reserved for Terlan 0.0.7; use ordinary trait impls for now",
                start,
                start + "impl Contract".len(),
            ));
        }
    }
    if let Some(start) = source.find("impl not ") {
        if source[start..]
            .split_once('.')
            .map_or(&source[start..], |(head, _)| head)
            .contains('{')
        {
            return Some(diagnostic(
                "negative trait impl declarations cannot have a body",
                start,
                start + "impl not".len(),
            ));
        }
    }
    if let Some(start) = source.find("=> Dynamic") {
        return Some(diagnostic(
            "implication target must be a closed structural field shape",
            start,
            start + "=> Dynamic".len(),
        ));
    }
    if let Some(start) = source.find("=> {}") {
        return Some(diagnostic(
            "implication target must contain at least one field",
            start,
            start + "=> {}".len(),
        ));
    }
    None
}

fn repeated_let_diagnostic(source: &str) -> Option<LalrpopBoundaryError> {
    if let Some(start) = source.find("let {}") {
        return Some(diagnostic(
            "refutable let group requires at least one binding",
            start,
            start + 6,
        ));
    }
    if let Some(start) = source.find("else {}") {
        return Some(diagnostic(
            "let else requires at least one fallback clause",
            start,
            start + 7,
        ));
    }
    if let Ok(tokens) = lex(source) {
        let significant = tokens
            .iter()
            .filter(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Comment
                        | TokenKind::DocComment
                        | TokenKind::DocBlockComment
                        | TokenKind::ModuleDocComment
                )
            })
            .collect::<Vec<_>>();
        let mut in_let_sequence = false;
        for (index, token) in significant.iter().enumerate() {
            match token.kind {
                TokenKind::Let => in_let_sequence = true,
                TokenKind::Dot => in_let_sequence = false,
                TokenKind::Semicolon if in_let_sequence => {
                    let candidate = significant.get(index + 1);
                    let equals = significant.get(index + 2);
                    if candidate.is_some_and(|candidate| {
                        matches!(candidate.kind, TokenKind::Atom | TokenKind::Var)
                    }) && equals.is_some_and(|equals| equals.kind == TokenKind::Equals)
                    {
                        let candidate = candidate.expect("checked implicit binding candidate");
                        return Some(diagnostic(
                            "subsequent local binding must start with `let`; insert `let` before this binding",
                            candidate.start,
                            candidate.end,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if let Some((start, close)) = refutable_let_group(source) {
        let body = &source[start + "let {".len()..close];
        if body.matches("<-").count() > 1 && has_top_level_comma(body) {
            return Some(diagnostic(
                "local bindings must be separated by `; let`, not commas",
                start,
                close + 1,
            ));
        }
        if source[close + 1..].trim_start().starts_with(';') {
            return Some(diagnostic(
                "refutable let group requires an else fallback",
                start,
                start + 5,
            ));
        }
    }
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("let ") {
            if comma_grouped_let_binding(trimmed) {
                let start = source.find(line).unwrap_or_default();
                return Some(diagnostic(
                    "local bindings must be separated by `; let`, not commas",
                    start,
                    start + line.len(),
                ));
            }
            if trimmed.contains('=') && trimmed.ends_with('.') && !trimmed.contains(';') {
                let start = source.find(line).unwrap_or_default();
                return Some(diagnostic(
                    "let expression requires an explicit result expression",
                    start,
                    start + line.len(),
                ));
            }
        }
    }
    None
}

/// Detects a second local binding after a top-level comma.
///
/// Commas and equality operators inside calls, collection literals, tuple
/// patterns, and comprehensions belong to the binding value. Only a comma at
/// the same delimiter depth as the first `let` assignment can introduce the
/// retired `let first = value, second = value` spelling.
fn comma_grouped_let_binding(source: &str) -> bool {
    let Ok(tokens) = lex(source) else {
        return false;
    };
    let significant = tokens
        .iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocComment
                    | TokenKind::DocBlockComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::EOF
            )
        })
        .collect::<Vec<_>>();
    let mut depth = 0_usize;
    let mut saw_assignment = false;
    let mut index = 0_usize;
    while index < significant.len() {
        match significant[index].kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::Equals if depth == 0 => saw_assignment = true,
            TokenKind::Comma if depth == 0 && saw_assignment => {
                let candidate = significant.get(index + 1);
                let equals = significant.get(index + 2);
                if candidate
                    .is_some_and(|token| matches!(token.kind, TokenKind::Atom | TokenKind::Var))
                    && equals.is_some_and(|token| token.kind == TokenKind::Equals)
                {
                    return true;
                }
            }
            _ => {}
        }
        index += 1;
    }
    false
}

fn significant_token_span(source: &str, text: &str) -> Option<(usize, usize)> {
    lex(source).ok()?.into_iter().find_map(|token| {
        (!matches!(
            token.kind,
            TokenKind::Comment
                | TokenKind::DocBlockComment
                | TokenKind::DocComment
                | TokenKind::ModuleDocComment
        ) && token.text == text)
            .then_some((token.start, token.end))
    })
}

fn trivia_spans(source: &str) -> Vec<(usize, usize)> {
    lex(source)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|token| {
            matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocBlockComment
                    | TokenKind::DocComment
                    | TokenKind::ModuleDocComment
            )
            .then_some((token.start, token.end))
        })
        .collect()
}

fn refutable_let_group(source: &str) -> Option<(usize, usize)> {
    let tokens = lex(source).ok()?;
    let significant = tokens
        .iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Comment
                    | TokenKind::DocBlockComment
                    | TokenKind::DocComment
                    | TokenKind::ModuleDocComment
                    | TokenKind::EOF
            )
        })
        .collect::<Vec<_>>();
    for index in 0..significant.len().saturating_sub(1) {
        if significant[index].kind != TokenKind::Let
            || significant[index + 1].kind != TokenKind::LBrace
        {
            continue;
        }
        let mut depth = 0_usize;
        let mut refutable = false;
        for token in &significant[index + 1..] {
            match token.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if refutable {
                            return Some((significant[index].start, token.start));
                        }
                        break;
                    }
                }
                TokenKind::LtMinus if depth == 1 => refutable = true,
                _ => {}
            }
        }
    }
    None
}

fn has_top_level_comma(source: &str) -> bool {
    let Ok(tokens) = lex(source) else {
        return false;
    };
    let mut depth = 0_usize;
    for token in tokens {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
            }
            TokenKind::Comma if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

fn duplicate_field_in_braces(source: &str) -> Option<usize> {
    if !source.contains("=>") && !source.lines().any(|line| line.contains(" type ")) {
        return None;
    }
    let Ok(tokens) = lex(source) else {
        return None;
    };
    let mut scopes = Vec::<Option<HashSet<String>>>::new();
    let mut previous_significant = None;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => {
                let tracked = scopes.last().is_some_and(Option::is_some)
                    || previous_significant.as_ref().is_some_and(|kind| {
                        matches!(kind, TokenKind::Equals | TokenKind::FatArrow)
                    });
                scopes.push(tracked.then(HashSet::new));
            }
            TokenKind::RBrace => {
                scopes.pop();
            }
            TokenKind::Atom | TokenKind::Var if scopes.last().is_some_and(Option::is_some) => {
                let next = tokens[index + 1..].iter().find(|candidate| {
                    !matches!(
                        candidate.kind,
                        TokenKind::Comment
                            | TokenKind::DocComment
                            | TokenKind::DocBlockComment
                            | TokenKind::ModuleDocComment
                    )
                });
                if next.is_some_and(|next| next.kind == TokenKind::Colon)
                    && !scopes
                        .last_mut()
                        .and_then(Option::as_mut)
                        .expect("tracked field scope")
                        .insert(token.text.clone())
                {
                    return Some(token.start);
                }
            }
            _ => {}
        }
        if !matches!(
            token.kind,
            TokenKind::Comment
                | TokenKind::DocComment
                | TokenKind::DocBlockComment
                | TokenKind::ModuleDocComment
        ) {
            previous_significant = Some(token.kind.clone());
        }
    }
    None
}

fn unsupported_when_guard(source: &str) -> Option<usize> {
    lex(source)
        .ok()?
        .into_iter()
        .find_map(|token| (token.kind == TokenKind::When).then_some(token.start))
}

fn concrete_higher_kind_slot(source: &str) -> Option<usize> {
    let marker = source.find("[M[")?;
    let slot = marker + 3;
    (source[slot..]
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase()))
    .then_some(slot)
}

fn is_reverse_alias_head(line: &str) -> bool {
    line.find('(').is_some_and(|open| {
        line[open + 1..].find(" = {").is_some_and(|alias| {
            line[open + 1..open + 1 + alias]
                .trim()
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
    })
}

fn unsupported_annotation_subject(line: &str) -> Option<usize> {
    let annotation = line.strip_prefix('@')?;
    let path_end = annotation
        .char_indices()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '.'
        })
        .last()
        .map_or(0, |(index, character)| index + character.len_utf8());
    let tail = annotation[path_end..].trim_start();
    if tail.is_empty() || tail.starts_with('{') {
        return None;
    }
    let subject_start = line.len() - tail.len();
    (tail.starts_with('"')
        || tail
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        || tail.contains('.'))
    .then_some(subject_start)
}

fn diagnostic(message: impl Into<String>, start: usize, end: usize) -> LalrpopBoundaryError {
    LalrpopBoundaryError {
        message: message.into(),
        span: Span::new(start, end),
    }
}
