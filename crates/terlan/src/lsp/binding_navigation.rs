use std::collections::{BTreeMap, BTreeSet};

use crate::terlan_syntax::{lex, token::Token, token::TokenKind, Span};
use crate::terlan_typeck::{BindingAnalysis, CoreBindingId, CoreBindingRegionId};

/// One source token resolved to an exact compiler binding identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BindingOccurrence {
    pub(crate) binding: CoreBindingId,
    pub(crate) region: CoreBindingRegionId,
    pub(crate) span: Span,
    pub(crate) declaration: bool,
}

/// Source-token projection of compiler-owned binding evidence.
#[derive(Debug, Clone, Default)]
pub(crate) struct BindingNavigationIndex {
    occurrences: Vec<BindingOccurrence>,
}

impl BindingNavigationIndex {
    /// Projects exact compiler binding evidence onto identifier tokens.
    pub(crate) fn build(text: &str, analysis: &BindingAnalysis) -> Self {
        let tokens = lex(text).unwrap_or_default();
        let mut occurrences = Vec::new();
        let mut used_tokens = BTreeSet::new();
        let mut bindings_by_name = BTreeMap::<&str, Vec<_>>::new();
        for binding in &analysis.evidence.bindings {
            bindings_by_name
                .entry(&binding.name)
                .or_default()
                .push(binding);
        }

        for (name, bindings) in &bindings_by_name {
            let matching = identifier_tokens(&tokens, name);
            let mut declarations = matching
                .iter()
                .copied()
                .filter(|index| looks_like_binding(&tokens, *index))
                .collect::<Vec<_>>();
            if declarations.len() < bindings.len() {
                let fallback = matching
                    .iter()
                    .copied()
                    .filter(|index| !declarations.contains(index))
                    .collect::<Vec<_>>();
                declarations.extend(fallback);
            }
            for (binding, token_index) in bindings.iter().zip(declarations) {
                let token = &tokens[token_index];
                used_tokens.insert(token_index);
                occurrences.push(BindingOccurrence {
                    binding: binding.id,
                    region: binding.region,
                    span: token.span(),
                    declaration: true,
                });
            }
        }

        let mut references_by_name = BTreeMap::<&str, Vec<_>>::new();
        for reference in &analysis.evidence.references {
            references_by_name
                .entry(&reference.name)
                .or_default()
                .push(reference);
        }
        for (name, references) in references_by_name {
            let candidates = identifier_tokens(&tokens, name)
                .into_iter()
                .filter(|index| !used_tokens.contains(index))
                .collect::<Vec<_>>();
            for (reference, token_index) in references.iter().zip(candidates) {
                let Some(binding) = analysis
                    .evidence
                    .bindings
                    .iter()
                    .find(|binding| binding.id == reference.binding)
                else {
                    continue;
                };
                occurrences.push(BindingOccurrence {
                    binding: reference.binding,
                    region: binding.region,
                    span: tokens[token_index].span(),
                    declaration: false,
                });
            }
        }
        occurrences.sort_by_key(|occurrence| (occurrence.span.start, occurrence.span.end));
        Self { occurrences }
    }

    pub(crate) fn occurrence_at(&self, byte_offset: usize) -> Option<BindingOccurrence> {
        self.occurrences.iter().copied().find(|occurrence| {
            occurrence.span.start <= byte_offset && byte_offset <= occurrence.span.end
        })
    }

    pub(crate) fn occurrences_for(&self, binding: CoreBindingId) -> Vec<BindingOccurrence> {
        self.occurrences
            .iter()
            .copied()
            .filter(|occurrence| occurrence.binding == binding)
            .collect()
    }

    pub(crate) fn declaration_for(&self, binding: CoreBindingId) -> Option<BindingOccurrence> {
        self.occurrences
            .iter()
            .copied()
            .find(|occurrence| occurrence.binding == binding && occurrence.declaration)
    }

    pub(crate) fn is_declaration(&self, span: Span) -> bool {
        self.occurrences.iter().any(|occurrence| {
            occurrence.declaration
                && occurrence.span.start == span.start
                && occurrence.span.end == span.end
        })
    }

    pub(crate) fn all(&self) -> &[BindingOccurrence] {
        &self.occurrences
    }
}

/// Finds the duplicate declaration token addressed by one compiler diagnostic.
pub(crate) fn duplicate_binding_replacement(
    text: &str,
    analysis: &BindingAnalysis,
    diagnostic_start: usize,
    diagnostic_end: usize,
    diagnostic_message: &str,
) -> Option<(Span, String)> {
    let collision = analysis.collisions.iter().find(|collision| {
        diagnostic_message.contains(&format!("`{}` is already bound", collision.name))
            && diagnostic_message.contains(&format!("use `{}`", collision.suggested_name))
    })?;
    let tokens = lex(text).ok()?;
    let mut declarations = identifier_tokens(&tokens, &collision.name)
        .into_iter()
        .filter(|index| looks_like_binding(&tokens, *index))
        .filter(|index| {
            let token = &tokens[*index];
            token.start >= diagnostic_start && token.end <= diagnostic_end
        })
        .collect::<Vec<_>>();
    declarations.sort_unstable();
    let target = declarations.last().copied()?;
    Some((tokens[target].span(), collision.suggested_name.clone()))
}

fn identifier_tokens(tokens: &[Token], name: &str) -> Vec<usize> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            (matches!(
                token.kind,
                TokenKind::Atom | TokenKind::Ident | TokenKind::Var
            ) && token.text == name)
                .then_some(index)
        })
        .collect()
}

fn looks_like_binding(tokens: &[Token], index: usize) -> bool {
    let previous = index.checked_sub(1).and_then(|item| tokens.get(item));
    let next = tokens.get(index + 1);
    if previous.is_some_and(|token| token.kind == TokenKind::Let)
        || next.is_some_and(|token| token.kind == TokenKind::LtMinus)
        || next.is_some_and(|token| token.kind == TokenKind::Arrow)
    {
        return true;
    }
    if next.is_some_and(|token| token.kind == TokenKind::Colon)
        && callable_arrow_ahead(tokens, index)
    {
        return true;
    }
    if previous.is_some_and(|token| token.kind == TokenKind::Equals)
        || next.is_some_and(|token| token.kind == TokenKind::Equals)
    {
        return callable_arrow_ahead(tokens, index);
    }
    callable_pattern_member(tokens, index)
}

fn callable_arrow_ahead(tokens: &[Token], index: usize) -> bool {
    for token in tokens.iter().skip(index + 1).take(32) {
        match token.kind {
            TokenKind::Arrow => return true,
            TokenKind::Dot | TokenKind::Semicolon => return false,
            _ => {}
        }
    }
    false
}

fn callable_pattern_member(tokens: &[Token], index: usize) -> bool {
    let mut depth = 0isize;
    for token in tokens.iter().skip(index + 1).take(24) {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::Arrow if depth <= 0 => return true,
            TokenKind::Dot | TokenKind::Semicolon if depth <= 0 => return false,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
#[path = "binding_navigation_test.rs"]
#[cfg(test)]
mod binding_navigation_test;
