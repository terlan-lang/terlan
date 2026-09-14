//! Function results must admit every possible branch, not just overlap the annotation.

use super::{apply_subst, pretty_type, unify, HashMap, Type, TypeVarId};

/// Checks all inferred return alternatives and commits substitutions only on success.
///
/// Ordinary unification also serves pattern matching, where a single matching
/// union member is sufficient. A return annotation instead constrains every
/// possible result. An unconstrained variable still captures the whole inferred
/// union, preserving generic inference rather than binding its first member.
pub(in super::super) fn unify_return_type(
    expected: &Type,
    actual: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let mut trial = subst.clone();
    let expected = apply_subst(expected, &trial);
    let actual = apply_subst(actual, &trial);
    if let Type::Union(variants) = &actual {
        if !matches!(expected, Type::Var(_)) {
            for variant in variants {
                unify_return_type(&expected, variant, &mut trial).map_err(|_| {
                    format!(
                        "expected {} found {}",
                        pretty_type(&expected),
                        pretty_type(&actual)
                    )
                })?;
            }
            *subst = trial;
            return Ok(());
        }
    }
    unify(&expected, &actual, &mut trial)?;
    *subst = trial;
    Ok(())
}
