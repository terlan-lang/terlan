use std::collections::HashMap;

use crate::terlan_syntax::{Token, TokenKind};

use super::normalize_type_param_name;

/// Parsed trait reference with type arguments.
///
/// Inputs:
/// - Trait name text such as `Show[User]`.
///
/// Output:
/// - Trait name and raw type-argument text.
///
/// Transformation:
/// - Splits the trait reference enough for later type parsing and generic
///   substitution.
#[derive(Debug, Clone)]
pub(crate) struct ParsedTraitInstance {
    pub(crate) name: String,
    pub(crate) type_args: Vec<String>,
}

/// Parses one trait instance from source text.
///
/// Inputs:
/// - `text`: trait reference text, such as `Show[User]`.
///
/// Output:
/// - Parsed trait instance when lexing and structural parsing succeed.
/// - `None` for malformed or empty references.
///
/// Transformation:
/// - Lexes the source text with the canonical syntax lexer and delegates token
///   grouping to the bracket-aware trait instance parser.
pub(crate) fn parse_trait_instance_from_text(text: &str) -> Option<ParsedTraitInstance> {
    let tokens = crate::terlan_syntax::lexer::lex(text).ok()?;
    parse_trait_instance(&tokens)
}

/// Builds a stable display key for a trait instance.
///
/// Inputs:
/// - `target`: parsed trait instance.
///
/// Output:
/// - Normalized trait key, or `None` when the trait name is empty.
///
/// Transformation:
/// - Normalizes type argument whitespace and renders `Trait[Arg, ...]` only
///   when type arguments are present.
pub(crate) fn trait_instance_key(target: &ParsedTraitInstance) -> Option<String> {
    if target.name.is_empty() {
        return None;
    }

    if target.type_args.is_empty() {
        Some(target.name.clone())
    } else {
        Some(format!(
            "{}[{}]",
            target.name,
            target
                .type_args
                .iter()
                .map(|arg| normalize_trait_type_text(arg))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// Normalizes trait type-expression text for stable comparison.
///
/// Inputs:
/// - `text`: source type-expression text.
///
/// Output:
/// - Text with whitespace runs collapsed to one space.
///
/// Transformation:
/// - Performs a syntax-light normalization suitable for diagnostics and
///   signature comparison before full type lowering.
pub(crate) fn normalize_trait_type_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Substitutes trait type parameters inside a type-expression string.
///
/// Inputs:
/// - `text`: type-expression text from a trait method signature.
/// - `params`: trait type-parameter names.
/// - `args`: concrete type arguments from an implemented trait reference.
///
/// Output:
/// - Normalized type-expression text after replacing matching type variables.
///
/// Transformation:
/// - Lexes the type text and replaces upper-case identifier tokens whose text
///   matches a trait type parameter. Punctuation and non-matching tokens are
///   preserved, then normalized for stable diagnostics and comparisons.
pub(crate) fn specialize_trait_type_text(text: &str, params: &[String], args: &[String]) -> String {
    if params.is_empty() || args.is_empty() {
        return normalize_trait_type_text(text);
    }

    let substitutions = params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (normalize_type_param_name(param), arg.as_str()))
        .collect::<HashMap<_, _>>();

    let Ok(tokens) = crate::terlan_syntax::lexer::lex(text) else {
        return normalize_trait_type_text(text);
    };

    let mut parts = Vec::new();
    for token in tokens {
        if token.kind == TokenKind::EOF {
            break;
        }
        if matches!(
            token.kind,
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment
        ) {
            continue;
        }
        if token.kind == TokenKind::Var {
            if let Some(replacement) = substitutions.get(&normalize_type_param_name(&token.text)) {
                parts.push((*replacement).to_string());
                continue;
            }
        }
        parts.push(token.text);
    }

    normalize_trait_type_text(&join_token_texts_from_strings(&parts))
}

/// Compares two type-expression texts using compact whitespace-insensitive form.
///
/// Inputs:
/// - `left`: first type text.
/// - `right`: second type text.
///
/// Output:
/// - `true` when both texts are equal after removing whitespace.
///
/// Transformation:
/// - Applies the same compacting strategy used by syntax diagnostics that only
///   need source-stable shape comparison before full type identity lowering.
pub(crate) fn trait_type_text_equal(left: &str, right: &str) -> bool {
    compact_trait_type_text(left) == compact_trait_type_text(right)
}

/// Parses one tokenized trait instance.
///
/// Inputs:
/// - `tokens`: lexer tokens for a trait reference.
///
/// Output:
/// - Parsed trait instance, including nested type argument text.
/// - `None` when the trait name is empty.
///
/// Transformation:
/// - Splits the leading trait name from bracketed type arguments, preserving
///   nested bracket/brace/paren groups inside each argument.
fn parse_trait_instance(tokens: &[Token]) -> Option<ParsedTraitInstance> {
    if tokens.is_empty() {
        return None;
    }

    let mut name_end = tokens.len();
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::LBracket {
            name_end = idx;
            break;
        }
    }

    let name = tokens[..name_end]
        .iter()
        .filter_map(|token| match token.kind {
            TokenKind::Comment | TokenKind::DocComment | TokenKind::ModuleDocComment => None,
            TokenKind::Dot => Some(".".to_string()),
            _ => Some(token.text.clone()),
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .trim_matches('.')
        .to_string();
    if name.is_empty() {
        return None;
    }

    let mut type_args = Vec::new();
    if name_end >= tokens.len() {
        return Some(ParsedTraitInstance { name, type_args });
    }

    let mut pos = name_end + 1;
    let mut depth = 0i32;
    let mut current = Vec::new();

    while pos < tokens.len() {
        let token = &tokens[pos];

        if token.kind == TokenKind::RBracket && depth == 0 {
            if !current.is_empty() {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
            }
            break;
        }

        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                depth += 1;
                current.push(token.clone());
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.saturating_sub(1);
                if depth >= 0 {
                    current.push(token.clone());
                }
            }
            TokenKind::Comma if depth == 0 => {
                type_args.push(
                    join_token_texts(&current)
                        .split_whitespace()
                        .collect::<String>(),
                );
                current.clear();
            }
            _ => current.push(token.clone()),
        }

        pos += 1;
    }

    Some(ParsedTraitInstance { name, type_args })
}

/// Joins lexer token text with spaces.
///
/// Inputs:
/// - `tokens`: token slice to render.
///
/// Output:
/// - Space-separated token text.
///
/// Transformation:
/// - Preserves token text while reintroducing one space between adjacent
///   tokens, allowing caller-specific whitespace compaction afterward.
fn join_token_texts(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Joins token-text strings for type normalization.
///
/// Inputs:
/// - `parts`: token text fragments.
///
/// Output:
/// - A space-separated string.
///
/// Transformation:
/// - Mirrors `join_token_texts` for callers that already own string fragments.
fn join_token_texts_from_strings(parts: &[String]) -> String {
    parts
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compacts all whitespace out of trait type text.
///
/// Inputs:
/// - `input`: type-expression text.
///
/// Output:
/// - Text without whitespace.
///
/// Transformation:
/// - Uses the compact comparison style expected by trait signature diagnostics.
fn compact_trait_type_text(input: &str) -> String {
    input.split_whitespace().collect::<String>()
}
