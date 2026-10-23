//! Alias-aware subtype checks share completed comparisons within one immutable alias scope.

use super::*;

// Expansion depth is part of the key because exhausting the existing depth
// limit may reject a comparison that succeeds with more expansion available.
type SubtypeMemo = HashMap<(Type, Type, usize), bool>;

/// Checks subtype compatibility with visible generic variance metadata.
///
/// Inputs:
/// - `lhs`: candidate subtype.
/// - `rhs`: expected supertype.
/// - `aliases`: visible type aliases carrying generic parameter variance.
///
/// Output:
/// - `true` when `lhs` can be assigned to `rhs` under primitive, structural,
///   alias-expanded, and variance-aware named-type rules.
pub(in crate::compiler::typeck) fn is_subtype_with_aliases(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    is_subtype_with_aliases_inner(lhs, rhs, aliases, 0, &mut HashMap::new())
}

/// Recurses through alias-aware subtype checks with a bounded expansion depth.
///
/// Inputs:
/// - `lhs` and `rhs`: current subtype pair.
/// - `aliases`: visible alias metadata.
/// - `depth`: alias expansion depth guard.
///
/// Output:
/// - `true` when the current pair is compatible.
fn is_subtype_with_aliases_inner(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
    memo: &mut SubtypeMemo,
) -> bool {
    if is_subtype(lhs, rhs) {
        return true;
    }
    let key = (lhs.clone(), rhs.clone(), depth);
    if let Some(result) = memo.get(&key) {
        return *result;
    }
    let result = is_subtype_with_aliases_uncached(lhs, rhs, aliases, depth, memo);
    memo.insert(key, result);
    result
}

fn is_subtype_with_aliases_uncached(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
    memo: &mut SubtypeMemo,
) -> bool {
    if let (Some(lhs), Some(rhs)) = (portable_list_element(lhs), portable_list_element(rhs)) {
        return is_subtype_with_aliases_inner(lhs, rhs, aliases, depth, memo);
    }
    if let Some(result) = option_representation_is_subtype(lhs, rhs, |lhs, rhs| {
        is_subtype_with_aliases_inner(lhs, rhs, aliases, depth, memo)
    }) {
        return result;
    }
    match (lhs, rhs) {
        (Type::Union(items), _) => {
            items
                .iter()
                .all(|item| is_subtype_with_aliases_inner(item, rhs, aliases, depth, memo))
                || expand_and_retry_subtype(lhs, rhs, aliases, depth, memo)
        }
        (_, Type::Union(items)) => {
            items
                .iter()
                .any(|item| is_subtype_with_aliases_inner(lhs, item, aliases, depth, memo))
                || expand_and_retry_subtype(lhs, rhs, aliases, depth, memo)
        }
        (Type::Tuple(lhs_items), Type::Tuple(rhs_items)) if lhs_items.len() == rhs_items.len() => {
            lhs_items.iter().zip(rhs_items).all(|(lhs_item, rhs_item)| {
                is_subtype_with_aliases_inner(lhs_item, rhs_item, aliases, depth, memo)
            })
        }
        (
            Type::FixedArray {
                size: lhs_size,
                elem: lhs_elem,
            },
            Type::FixedArray {
                size: rhs_size,
                elem: rhs_elem,
            },
        ) => {
            lhs_size == rhs_size
                && is_subtype_with_aliases_inner(lhs_elem, rhs_elem, aliases, depth, memo)
        }
        (Type::Map(lhs_fields), Type::Map(rhs_fields)) => {
            super::map_fields::map_fields_is_subtype_with_aliases(
                lhs_fields,
                rhs_fields,
                |lhs, rhs| is_subtype_with_aliases_inner(lhs, rhs, aliases, depth, memo),
            )
        }
        (
            Type::Named {
                module: lhs_module,
                name: lhs_name,
                args: lhs_args,
            },
            Type::Named {
                module: rhs_module,
                name: rhs_name,
                args: rhs_args,
            },
        ) if lhs_module == rhs_module
            && lhs_name == rhs_name
            && lhs_args.len() == rhs_args.len() =>
        {
            named_type_args_are_subtypes(
                lhs_module, lhs_name, lhs_args, rhs_args, aliases, depth, memo,
            )
        }
        (
            Type::Function {
                params: lhs_params,
                ret: lhs_ret,
            },
            Type::Function {
                params: rhs_params,
                ret: rhs_ret,
            },
        ) if lhs_params.len() == rhs_params.len() => {
            lhs_params
                .iter()
                .zip(rhs_params.iter())
                .all(|(lhs_param, rhs_param)| {
                    is_subtype_with_aliases_inner(rhs_param, lhs_param, aliases, depth, memo)
                })
                && is_subtype_with_aliases_inner(lhs_ret, rhs_ret, aliases, depth, memo)
        }
        _ => expand_and_retry_subtype(lhs, rhs, aliases, depth, memo),
    }
}

/// Compares named generic arguments using the named type's declared variance.
///
/// Inputs:
/// - `module` and `name`: named type identity.
/// - `lhs_args` and `rhs_args`: applied generic arguments.
/// - `aliases`: visible alias metadata.
/// - `depth`: current subtype recursion depth.
///
/// Output:
/// - `true` when every argument pair satisfies the declared variance.
///
/// Transformation:
/// - Defaults to invariant when no metadata exists, so old generic types keep
///   conservative behavior until they declare `+` or `-`.
fn named_type_args_are_subtypes(
    module: &Option<String>,
    name: &str,
    lhs_args: &[Type],
    rhs_args: &[Type],
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
    memo: &mut SubtypeMemo,
) -> bool {
    let variance = named_type_variance(module, name, aliases);
    lhs_args
        .iter()
        .zip(rhs_args.iter())
        .enumerate()
        .all(|(index, (lhs_arg, rhs_arg))| {
            let variance = variance
                .and_then(|entries| entries.get(index))
                .copied()
                .unwrap_or(Variance::Invariant);
            type_args_match_variance(lhs_arg, rhs_arg, variance, aliases, depth, memo)
        })
}

/// Resolves declared variance for a named type from alias metadata.
///
/// Inputs:
/// - `module` and `name`: named type identity from a `Type::Named`.
/// - `aliases`: visible alias metadata.
///
/// Output:
/// - Slice of variance entries when the named type has visible metadata.
///
/// Transformation:
/// - Prefers a fully qualified alias key, then falls back to the unqualified
///   name for local aliases.
fn named_type_variance<'a>(
    module: &Option<String>,
    name: &str,
    aliases: &'a HashMap<String, TypeAlias>,
) -> Option<&'a [Variance]> {
    if let Some(module) = module {
        let qualified = format!("{}.{}", module, name);
        if let Some(alias) = aliases.get(&qualified) {
            return Some(&alias.param_variance);
        }
    }
    aliases
        .get(name)
        .map(|alias| alias.param_variance.as_slice())
}

/// Checks one generic argument pair under a variance direction.
///
/// Inputs:
/// - `lhs_arg` and `rhs_arg`: candidate and expected generic arguments.
/// - `variance`: declared variance for the parameter.
/// - `aliases`: visible alias metadata.
/// - `depth`: current subtype recursion depth.
///
/// Output:
/// - `true` when the pair satisfies the variance direction.
///
/// Transformation:
/// - Covariance preserves direction, contravariance reverses direction, and
///   invariance requires mutual assignability.
fn type_args_match_variance(
    lhs_arg: &Type,
    rhs_arg: &Type,
    variance: Variance,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
    memo: &mut SubtypeMemo,
) -> bool {
    match variance {
        Variance::Covariant => {
            is_subtype_with_aliases_inner(lhs_arg, rhs_arg, aliases, depth, memo)
        }
        Variance::Contravariant => {
            is_subtype_with_aliases_inner(rhs_arg, lhs_arg, aliases, depth, memo)
        }
        Variance::Invariant => {
            is_subtype_with_aliases_inner(lhs_arg, rhs_arg, aliases, depth, memo)
                && is_subtype_with_aliases_inner(rhs_arg, lhs_arg, aliases, depth, memo)
        }
    }
}

/// Retries subtype checking after visible alias expansion.
///
/// Inputs:
/// - `lhs` and `rhs`: current subtype pair.
/// - `aliases`: visible alias metadata.
/// - `depth`: current expansion depth.
///
/// Output:
/// - `true` when expanding at least one side makes the pair compatible.
///
/// Transformation:
/// - Expands only root non-opaque aliases with a conservative depth guard.
///   Structural containers recurse independently so a recursive alias is not
///   unrolled one additional level on every retry.
fn expand_and_retry_subtype(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
    memo: &mut SubtypeMemo,
) -> bool {
    if depth >= 8 {
        return false;
    }
    let lhs_expanded = expand_root_type_alias(lhs, aliases);
    let rhs_expanded = expand_root_type_alias(rhs, aliases);
    if lhs_expanded == *lhs && rhs_expanded == *rhs {
        return false;
    }
    is_subtype_with_aliases_inner(&lhs_expanded, &rhs_expanded, aliases, depth + 1, memo)
}

/// Expands an alias only when it is the root of the compared type.
fn expand_root_type_alias(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    if matches!(ty, Type::Named { .. }) {
        expand_type_aliases(ty, aliases)
    } else {
        ty.clone()
    }
}

#[cfg(test)]
#[path = "alias_subtyping_test.rs"]
mod tests;
