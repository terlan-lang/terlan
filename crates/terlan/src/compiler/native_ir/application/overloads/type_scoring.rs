//! Structural compatibility scoring for static overload selection.

use super::{AliasBodies, CoreType};

/// Scores structural compatibility, preferring exact nested type matches.
pub(super) fn type_match_score(
    expected: &CoreType,
    actual: &CoreType,
    aliases: &AliasBodies,
) -> Option<usize> {
    type_match_score_at(expected, actual, aliases, 0)
}

/// Scores structural compatibility while resolving bounded transparent aliases.
fn type_match_score_at(
    expected: &CoreType,
    actual: &CoreType,
    aliases: &AliasBodies,
    depth: usize,
) -> Option<usize> {
    if expected == actual {
        return Some(8);
    }
    if depth < 16 {
        if let CoreType::Named(name) = expected {
            if let Some(body) = aliases
                .get(name)
                .or_else(|| aliases.get(name.rsplit('.').next().unwrap_or(name)))
            {
                return type_match_score_at(body, actual, aliases, depth + 1)
                    .map(|score| score.saturating_sub(1));
            }
        }
        if let CoreType::Named(name) = actual {
            if let Some(body) = aliases
                .get(name)
                .or_else(|| aliases.get(name.rsplit('.').next().unwrap_or(name)))
            {
                return type_match_score_at(expected, body, aliases, depth + 1)
                    .map(|score| score.saturating_sub(1));
            }
        }
    }
    match (expected, actual) {
        (CoreType::Dynamic | CoreType::Term, _) | (_, CoreType::Dynamic) => Some(1),
        (CoreType::Number, CoreType::Int | CoreType::Float | CoreType::Number) => Some(2),
        (CoreType::Atom, CoreType::AtomLiteral(_)) => Some(4),
        (CoreType::List(expected), CoreType::List(actual)) => {
            type_match_score_at(expected, actual, aliases, depth).map(|score| score + 4)
        }
        (
            CoreType::Apply {
                constructor: expected_constructor,
                args: expected_args,
            },
            CoreType::Apply {
                constructor: actual_constructor,
                args: actual_args,
            },
        ) if expected_constructor == actual_constructor
            && expected_args.len() == actual_args.len() =>
        {
            let mut score = 4;
            for (expected, actual) in expected_args.iter().zip(actual_args) {
                score += type_match_score_at(expected, actual, aliases, depth)?;
            }
            Some(score)
        }
        _ => None,
    }
}
