//! Structural compatibility scoring for static overload selection.

use super::{core_type_contract_text, AliasBodies, CoreType};

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
        if let Some(body) = expand_alias(expected, aliases) {
            return type_match_score_at(&body, actual, aliases, depth + 1)
                .map(|score| score.saturating_sub(1));
        }
        if let Some(body) = expand_alias(actual, aliases) {
            return type_match_score_at(expected, &body, aliases, depth + 1)
                .map(|score| score.saturating_sub(1));
        }
    }
    match (expected, actual) {
        (CoreType::Dynamic | CoreType::Term, _) | (_, CoreType::Dynamic) => Some(1),
        (CoreType::Number, CoreType::Int | CoreType::Float | CoreType::Number) => Some(2),
        (CoreType::Atom, CoreType::AtomLiteral(_)) => Some(4),
        (CoreType::Union(expected), CoreType::Union(actual)) if expected.len() == actual.len() => {
            let mut score = 4;
            for actual_variant in actual {
                score += expected
                    .iter()
                    .filter_map(|expected_variant| {
                        type_match_score_at(expected_variant, actual_variant, aliases, depth + 1)
                    })
                    .max()?;
            }
            Some(score)
        }
        (CoreType::Union(variants), actual) => variants
            .iter()
            .filter_map(|variant| type_match_score_at(variant, actual, aliases, depth + 1))
            .max()
            .map(|score| score + 2),
        (CoreType::List(expected), CoreType::List(actual)) => {
            type_match_score_at(expected, actual, aliases, depth).map(|score| score + 4)
        }
        (CoreType::Tuple(expected), CoreType::Tuple(actual)) if expected.len() == actual.len() => {
            let mut score = 4;
            for (expected, actual) in expected.iter().zip(actual) {
                let expected = match expected {
                    crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
                    | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                let actual = match actual {
                    crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
                    | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => ty,
                };
                score += type_match_score_at(expected, actual, aliases, depth + 1)?;
            }
            Some(score)
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

/// Expands one transparent alias application with its concrete arguments.
fn expand_alias(ty: &CoreType, aliases: &AliasBodies) -> Option<CoreType> {
    let (name, args) = match ty {
        CoreType::Named(name) => (name, &[][..]),
        CoreType::Apply { constructor, args } => (constructor, args.as_slice()),
        _ => return None,
    };
    let alias = aliases
        .get(name)
        .or_else(|| aliases.get(name.rsplit('.').next().unwrap_or(name)))?;
    (alias.params.len() == args.len()).then(|| {
        let values = alias
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();
        crate::compiler::native_ir::generic_specialization::substitute(
            &alias.body,
            &alias.params,
            &values,
        )
    })
}

/// Renders inferred call arguments while preserving unknown positions.
pub(super) fn argument_contract(arguments: &[Option<CoreType>]) -> String {
    arguments
        .iter()
        .map(|argument| core_type_contract_text(argument.as_ref()))
        .collect::<Vec<_>>()
        .join(",")
}
