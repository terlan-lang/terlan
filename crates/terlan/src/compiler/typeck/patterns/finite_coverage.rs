//! Pattern-directed refinement of finite fields without eager Cartesian products.

use super::*;

const MAX_REGIONS: usize = 4096;
const MAX_REFINEMENTS: usize = 65536;
const MAX_DEPTH: usize = 256;

#[cfg(test)]
#[path = "finite_coverage_test.rs"]
mod tests;

/// Distinguishes analysis exhaustion from excessive pattern nesting.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::compiler::typeck) enum FiniteCoverageError {
    AnalysisBudget,
    NestingBudget,
}

impl std::fmt::Display for FiniteCoverageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AnalysisBudget => {
                "finite pattern coverage exceeds its analysis budget; add a covering pattern"
            }
            Self::NestingBudget => "finite pattern coverage exceeds its nesting budget",
        })
    }
}

impl std::error::Error for FiniteCoverageError {}

/// Identifies finite products that need checking even when their outer type is one tuple.
pub(in crate::compiler::typeck) fn has_finite_fields(ty: &Type) -> bool {
    match ty {
        Type::Bool | Type::Union(_) => true,
        Type::Tuple(items) => items.iter().any(has_finite_fields),
        _ => false,
    }
}

/// Subtracts one unguarded pattern, refining only the finite coordinates it tests.
/// Resource exhaustion is an error, never evidence that an unmatched region is covered.
pub(in crate::compiler::typeck) fn subtract_finite_pattern(
    remaining: Vec<Type>,
    pattern: &SyntaxPatternOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> Result<Vec<Type>, FiniteCoverageError> {
    let mut pending = remaining.into_iter().rev().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut steps = 0;
    while let Some(region) = pending.pop() {
        steps += 1;
        if steps > MAX_REFINEMENTS || pending.len() + output.len() >= MAX_REGIONS {
            return Err(FiniteCoverageError::AnalysisBudget);
        }
        if covers(pattern, &region, aliases) {
            continue;
        }
        if let Some((left, right)) = refine(pattern, &region, 0)? {
            pending.push(right);
            pending.push(left);
        } else {
            output.push(region);
        }
    }
    Ok(output)
}

fn nested_pattern(pattern: &SyntaxPatternOutput) -> Option<&SyntaxPatternOutput> {
    (pattern.kind == SyntaxPatternKind::Alias
        || (pattern.kind == SyntaxPatternKind::Constructor
            && pattern
                .text
                .as_deref()
                .is_some_and(|name| name.starts_with("$const:"))))
    .then(|| pattern.children.first())
    .flatten()
}

fn tuple_children<'a>(
    pattern: &'a SyntaxPatternOutput,
    items: &[Type],
) -> Option<(usize, &'a [SyntaxPatternOutput])> {
    match pattern.kind {
        SyntaxPatternKind::Tuple if pattern.children.len() == items.len() => {
            Some((0, &pattern.children))
        }
        SyntaxPatternKind::Constructor => {
            let Some(Type::LiteralAtom(head)) = items.first() else {
                return None;
            };
            let name = pattern.text.as_deref()?;
            (constructor_pattern_atom_name(name) == *head
                && pattern.children.len() + 1 == items.len())
            .then_some((1, &pattern.children))
        }
        _ => None,
    }
}

fn covers(pattern: &SyntaxPatternOutput, ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    if let Some(child) = nested_pattern(pattern) {
        return covers(child, ty, aliases);
    }
    if let Type::Union(items) = ty {
        return items.iter().all(|item| covers(pattern, item, aliases));
    }
    if let Type::Tuple(items) = ty {
        if let Some((offset, children)) = tuple_children(pattern, items) {
            return children
                .iter()
                .zip(&items[offset..])
                .all(|(child, ty)| covers(child, ty, aliases));
        }
    }
    syntax_pattern_subsumes_variant(pattern, ty, aliases)
}

fn disjoint(pattern: &SyntaxPatternOutput, ty: &Type) -> bool {
    if let Some(child) = nested_pattern(pattern) {
        return disjoint(child, ty);
    }
    match ty {
        Type::LiteralAtom(atom) if pattern.kind == SyntaxPatternKind::Atom => {
            pattern.text.as_ref().is_some_and(|value| value != atom)
        }
        Type::LiteralAtom(atom)
            if pattern.kind == SyntaxPatternKind::Constructor && pattern.children.is_empty() =>
        {
            pattern
                .text
                .as_deref()
                .is_some_and(|name| constructor_pattern_atom_name(name) != *atom)
        }
        Type::Tuple(items) => tuple_children(pattern, items).is_some_and(|(offset, children)| {
            children
                .iter()
                .zip(&items[offset..])
                .any(|(child, ty)| disjoint(child, ty))
        }),
        Type::Union(items) => items.iter().all(|item| disjoint(pattern, item)),
        _ => false,
    }
}

fn refine(
    pattern: &SyntaxPatternOutput,
    ty: &Type,
    depth: usize,
) -> Result<Option<(Type, Type)>, FiniteCoverageError> {
    if depth > MAX_DEPTH {
        return Err(FiniteCoverageError::NestingBudget);
    }
    if let Some(child) = nested_pattern(pattern) {
        return refine(child, ty, depth + 1);
    }
    if disjoint(pattern, ty) {
        return Ok(None);
    }
    match ty {
        Type::Bool
            if pattern.kind == SyntaxPatternKind::Atom
                && matches!(pattern.text.as_deref(), Some("true" | "false")) =>
        {
            Ok(Some((
                Type::LiteralAtom("true".to_owned()),
                Type::LiteralAtom("false".to_owned()),
            )))
        }
        Type::Union(items) if items.len() > 1 => Ok(Some((
            items[0].clone(),
            if items.len() == 2 {
                items[1].clone()
            } else {
                Type::Union(items[1..].to_vec())
            },
        ))),
        Type::Tuple(items) => {
            if let Some((offset, children)) = tuple_children(pattern, items) {
                for (index, child) in children.iter().enumerate() {
                    let index = offset + index;
                    if let Some((left, right)) = refine(child, &items[index], depth + 1)? {
                        let mut left_items = items.clone();
                        let mut right_items = items.clone();
                        left_items[index] = left;
                        right_items[index] = right;
                        return Ok(Some((Type::Tuple(left_items), Type::Tuple(right_items))));
                    }
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}
