use std::collections::HashMap;

use crate::terlan_hir::{ConstructorSignature, FunctionSignature, FunctionSymbol, ModuleInterface};

mod builtins;
mod interface;
mod map_fields;
pub(crate) mod parser;
mod special;
mod substitution;
mod text;

pub(super) use builtins::{
    builtin_call, is_literal_atom, is_removed_implicit_builtin_call,
    widen_list_literal_element_type,
};
pub(super) use interface::{
    expand_interface_global_aliases, interface_qualified_type_names, interface_type_aliases,
    interface_type_names, parse_interface_constructor_schemes, parse_interface_signature,
    parse_symbol_scheme, qualify_type_names,
};
pub(super) use map_fields::{
    map_fields_is_subtype, map_fields_is_subtype_with_aliases, unify_map_fields,
};
pub(super) use parser::{
    alias_constructor_param_names_from_variants, is_map_type, parse_type_expr, split_named_type,
};
pub(super) use special::{
    are_template_html_equivalent_types, are_unit_equivalent_types, is_unit_literal_type,
    is_unit_named_type,
};
pub(super) use substitution::{
    existential_types_are_alpha_equivalent, expand_type_aliases, mapping_without_bound_params,
    substitute_type_vars,
};
pub(super) use text::{
    compact_spaces, split_module_name, split_top_level_csv, split_top_level_plus,
};

use crate::terlan_typeck::{
    pretty_type, ConstructorScheme, MapFieldType, QualifiedTypeName, Type, TypeAlias, TypeVarId,
    Variance,
};

/// Normalizes a set of union variants.
///
/// Inputs:
/// - `types`: candidate union variants that may include nested unions.
///
/// Output:
/// - `Never` for an empty union, a single type for singleton unions, or a
///   deduplicated `Union`.
///
/// Transformation:
/// - Flattens nested unions, removes `Never`, short-circuits on `Term`, and
///   drops variants covered by wider supertypes.
pub(super) fn normalize_union(mut types: Vec<Type>) -> Type {
    let mut expanded = Vec::new();
    while let Some(ty) = types.pop() {
        match ty {
            Type::Union(items) => expanded.extend(items),
            other => expanded.push(other),
        }
    }

    let mut normalized: Vec<Type> = Vec::new();
    for candidate in expanded {
        if candidate == Type::Never {
            continue;
        }
        if candidate == Type::Term {
            return Type::Term;
        }
        if normalized
            .iter()
            .any(|existing| is_subtype(&candidate, existing))
        {
            continue;
        }
        normalized.retain(|existing| !is_subtype(existing, &candidate));
        normalized.push(candidate);
    }

    if normalized.is_empty() {
        Type::Never
    } else if normalized.len() == 1 {
        normalized.into_iter().next().unwrap()
    } else {
        Type::Union(normalized)
    }
}

/// Checks the current structural subtype relation.
///
/// Inputs:
/// - `lhs`: candidate subtype.
/// - `rhs`: expected supertype.
///
/// Output:
/// - `true` when `lhs` is assignable to `rhs` without conversion.
///
/// Transformation:
/// - Applies primitive widening, literal widening, Unit equivalence, fixed-array
///   compatibility, and structural map-field compatibility.
pub(super) fn is_subtype(lhs: &Type, rhs: &Type) -> bool {
    if lhs == rhs {
        return true;
    }
    if existential_types_are_alpha_equivalent(lhs, rhs) {
        return true;
    }
    if are_unit_equivalent_types(lhs, rhs) {
        return true;
    }
    if are_template_html_equivalent_types(lhs, rhs) {
        return true;
    }
    match (lhs, rhs) {
        (_, Type::Dynamic) => true,
        (_, Type::Term) => true,
        (Type::Int, Type::Number) => true,
        (Type::Float, Type::Number) => true,
        (Type::LiteralInt(_), Type::Int) => true,
        (Type::LiteralInt(_), Type::Number) => true,
        (Type::LiteralAtom(_), Type::Atom) => true,
        (
            Type::FixedArray {
                size: lhs_size,
                elem: lhs_elem,
            },
            Type::FixedArray {
                size: rhs_size,
                elem: rhs_elem,
            },
        ) => lhs_size == rhs_size && is_subtype(lhs_elem, rhs_elem),
        (Type::Map(lhs), Type::Map(rhs)) => map_fields_is_subtype(lhs, rhs),
        (Type::Never, _) => true,
        _ => false,
    }
}

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
///
/// Transformation:
/// - Runs the existing structural subtype relation first, then uses alias
///   metadata to compare generic arguments covariantly, contravariantly, or
///   invariantly. Non-opaque aliases are expanded as a final fallback so
///   structural aliases still behave like their bodies.
pub(super) fn is_subtype_with_aliases(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    is_subtype_with_aliases_inner(lhs, rhs, aliases, 0)
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
///
/// Transformation:
/// - Applies primitive checks, union distribution, container variance, named
///   generic variance, and then a guarded alias expansion fallback.
fn is_subtype_with_aliases_inner(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
) -> bool {
    if is_subtype(lhs, rhs) {
        return true;
    }
    match (lhs, rhs) {
        (Type::Union(items), _) => {
            items
                .iter()
                .all(|item| is_subtype_with_aliases_inner(item, rhs, aliases, depth))
                || expand_and_retry_subtype(lhs, rhs, aliases, depth)
        }
        (_, Type::Union(items)) => {
            items
                .iter()
                .any(|item| is_subtype_with_aliases_inner(lhs, item, aliases, depth))
                || expand_and_retry_subtype(lhs, rhs, aliases, depth)
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
                && is_subtype_with_aliases_inner(lhs_elem, rhs_elem, aliases, depth)
        }
        (Type::Map(lhs_fields), Type::Map(rhs_fields)) => {
            map_fields_is_subtype_with_aliases(lhs_fields, rhs_fields, aliases, depth)
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
            named_type_args_are_subtypes(lhs_module, lhs_name, lhs_args, rhs_args, aliases, depth)
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
                    is_subtype_with_aliases_inner(rhs_param, lhs_param, aliases, depth)
                })
                && is_subtype_with_aliases_inner(lhs_ret, rhs_ret, aliases, depth)
        }
        _ => expand_and_retry_subtype(lhs, rhs, aliases, depth),
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
            type_args_match_variance(lhs_arg, rhs_arg, variance, aliases, depth)
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
) -> bool {
    match variance {
        Variance::Covariant => is_subtype_with_aliases_inner(lhs_arg, rhs_arg, aliases, depth),
        Variance::Contravariant => is_subtype_with_aliases_inner(rhs_arg, lhs_arg, aliases, depth),
        Variance::Invariant => {
            is_subtype_with_aliases_inner(lhs_arg, rhs_arg, aliases, depth)
                && is_subtype_with_aliases_inner(rhs_arg, lhs_arg, aliases, depth)
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
/// - Expands non-opaque aliases with a conservative depth guard to avoid
///   recursive alias cycles from making subtype checks non-terminating.
fn expand_and_retry_subtype(
    lhs: &Type,
    rhs: &Type,
    aliases: &HashMap<String, TypeAlias>,
    depth: usize,
) -> bool {
    if depth >= 8 {
        return false;
    }
    let lhs_expanded = expand_type_aliases(lhs, aliases);
    let rhs_expanded = expand_type_aliases(rhs, aliases);
    if lhs_expanded == *lhs && rhs_expanded == *rhs {
        return false;
    }
    is_subtype_with_aliases_inner(&lhs_expanded, &rhs_expanded, aliases, depth + 1)
}

/// Unifies two types and updates type-variable substitutions.
///
/// Inputs:
/// - `left`: first type constraint.
/// - `right`: second type constraint.
/// - `subst`: mutable substitution table for type variables.
///
/// Output:
/// - `Ok(())` when the types can be made compatible.
/// - `Err(message)` with a human-readable mismatch when unification fails.
///
/// Transformation:
/// - Applies existing substitutions, binds variables with occurs checks, and
///   recursively unifies composite type structure.
pub(super) fn unify(
    left: &Type,
    right: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let left = apply_subst(left, subst);
    let right = apply_subst(right, subst);

    if are_unit_equivalent_types(&left, &right) {
        return Ok(());
    }
    if are_template_html_equivalent_types(&left, &right) {
        return Ok(());
    }

    match (&left, &right) {
        (Type::Dynamic, _) | (_, Type::Dynamic) => Ok(()),
        (Type::Placeholder, Type::Placeholder) => Ok(()),
        (Type::Term, _) => Ok(()),
        (_, Type::Never) => Ok(()),
        (Type::Var(left_id), Type::Var(right_id)) if left_id == right_id => Ok(()),
        (Type::Var(id), rhs) => bind_var(*id, rhs.clone(), subst),
        (lhs, Type::Var(id)) => bind_var(*id, lhs.clone(), subst),
        (Type::Existential { .. }, Type::Existential { .. }) => {
            if existential_types_are_alpha_equivalent(&left, &right) {
                Ok(())
            } else {
                Err(format!(
                    "expected {} found {}",
                    pretty_type(&left),
                    pretty_type(&right)
                ))
            }
        }
        (Type::Union(left), Type::Union(right)) => {
            for l in left {
                let mut trial_ok = false;
                for r in right {
                    let mut trial_subst = subst.clone();
                    if unify(l, r, &mut trial_subst).is_ok() {
                        *subst = trial_subst;
                        trial_ok = true;
                        break;
                    }
                }
                if !trial_ok {
                    return Err(format!(
                        "expected {} but could not match {}",
                        pretty_type(&Type::Union(right.to_vec())),
                        pretty_type(l)
                    ));
                }
            }
            Ok(())
        }
        (Type::Union(left), rhs) => {
            for l in left {
                let mut trial_subst = subst.clone();
                if unify(l, rhs, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(&Type::Union(left.clone())),
                pretty_type(rhs)
            ))
        }
        (lhs, Type::Union(right)) => {
            for r in right {
                let mut trial_subst = subst.clone();
                if unify(lhs, r, &mut trial_subst).is_ok() {
                    *subst = trial_subst;
                    return Ok(());
                }
            }
            Err(format!(
                "expected {} found {}",
                pretty_type(lhs),
                pretty_type(&Type::Union(right.clone()))
            ))
        }
        (Type::Int, Type::Number) => Ok(()),
        (Type::Float, Type::Number) => Ok(()),
        (Type::LiteralInt(_), Type::Number) => Ok(()),
        (Type::Number, Type::LiteralInt(_)) => Ok(()),
        (Type::Number, Type::Int) | (Type::Number, Type::Float) => {
            Err("expected Number but found Int/Float".to_string())
        }
        (Type::LiteralAtom(left_atom), Type::LiteralAtom(right_atom))
            if left_atom == right_atom =>
        {
            Ok(())
        }
        (Type::LiteralInt(left_int), Type::LiteralInt(right_int)) if left_int == right_int => {
            Ok(())
        }
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Number, Type::Number)
        | (Type::Atom, Type::Atom)
        | (Type::Atom, Type::LiteralAtom(_))
        | (Type::LiteralAtom(_), Type::Atom)
        | (Type::Int, Type::LiteralInt(_))
        | (Type::LiteralInt(_), Type::Int)
        | (Type::Binary, Type::Binary)
        | (Type::Bool, Type::Bool) => Ok(()),
        (Type::List(lhs), Type::List(rhs)) => unify(lhs, rhs, subst),
        (Type::Map(lhs_fields), Type::Map(rhs_fields)) => {
            unify_map_fields(lhs_fields, rhs_fields, subst)
        }
        (Type::Tuple(lhs), Type::Tuple(rhs)) => {
            if lhs.len() != rhs.len() {
                return Err(format!(
                    "tuple arity mismatch: expected {} elements, found {}",
                    lhs.len(),
                    rhs.len()
                ));
            }
            for (left_item, right_item) in lhs.iter().zip(rhs.iter()) {
                unify(left_item, right_item, subst)?;
            }
            Ok(())
        }
        (
            Type::Named {
                module: m1,
                name: n1,
                args: args1,
            },
            Type::Named {
                module: m2,
                name: n2,
                args: args2,
            },
        ) => {
            if m1 == m2 && n1 == n2 && args1.len() == args2.len() {
                for (a, b) in args1.iter().zip(args2.iter()) {
                    unify(a, b, subst)?;
                }
                Ok(())
            } else {
                Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::Named {
                        module: m1.clone(),
                        name: n1.clone(),
                        args: args1.clone(),
                    }),
                    pretty_type(&Type::Named {
                        module: m2.clone(),
                        name: n2.clone(),
                        args: args2.clone(),
                    })
                ))
            }
        }
        (
            Type::Apply {
                constructor: left_constructor,
                args: left_args,
            },
            Type::Apply {
                constructor: right_constructor,
                args: right_args,
            },
        ) => {
            if left_constructor == right_constructor && left_args.len() == right_args.len() {
                for (left_arg, right_arg) in left_args.iter().zip(right_args.iter()) {
                    unify(left_arg, right_arg, subst)?;
                }
                Ok(())
            } else {
                Err(format!(
                    "expected {} found {}",
                    pretty_type(&left),
                    pretty_type(&right)
                ))
            }
        }
        (
            Type::Function {
                params: params_a,
                ret: ret_a,
            },
            Type::Function {
                params: params_b,
                ret: ret_b,
            },
        ) => {
            if params_a.len() != params_b.len() {
                return Err(format!(
                    "function arity mismatch: expected {} args, found {}",
                    params_a.len(),
                    params_b.len()
                ));
            }
            for (a, b) in params_a.iter().zip(params_b.iter()) {
                unify(a, b, subst)?;
            }
            unify(ret_a.as_ref(), ret_b.as_ref(), subst)
        }
        (
            Type::FixedArray {
                size: size_a,
                elem: elem_a,
            },
            Type::FixedArray {
                size: size_b,
                elem: elem_b,
            },
        ) => {
            if size_a != size_b {
                return Err(format!(
                    "expected {} found {}",
                    pretty_type(&Type::FixedArray {
                        size: *size_a,
                        elem: elem_a.clone(),
                    }),
                    pretty_type(&Type::FixedArray {
                        size: *size_b,
                        elem: elem_b.clone(),
                    })
                ));
            }
            unify(elem_a, elem_b, subst)
        }
        _ => Err(format!(
            "expected {} found {}",
            pretty_type(&left),
            pretty_type(&right)
        )),
    }
}

/// Binds a type variable to a concrete type.
///
/// Inputs:
/// - `id`: variable id being constrained.
/// - `value`: type to bind after generic literal widening.
/// - `subst`: mutable substitution table.
///
/// Output:
/// - `Ok(())` when the binding is accepted.
/// - `Err(message)` for recursive bindings or incompatible existing bindings.
///
/// Transformation:
/// - Widens literal bindings, unifies with any existing binding, checks occurs,
///   and records the substitution.
pub(super) fn bind_var(
    id: TypeVarId,
    value: Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let value = widen_type_var_binding(value);
    if let Some(existing) = subst.get(&id).cloned() {
        return unify(&existing, &value, subst);
    }
    if occurs(id, &value, subst) {
        return Err("recursive type".to_string());
    }
    subst.insert(id, value);
    Ok(())
}

/// Widens overly specific literal types when binding generic variables.
///
/// Inputs:
/// - `value`: inferred type about to bind a type variable.
///
/// Output:
/// - A type suitable for reuse across generic call arguments.
///
/// Transformation:
/// - Converts integer literal singleton types into `Int` so generic calls such
///   as `Some(1)` and `Some(2)` can agree on `T = Int`; leaves atom literals
///   unchanged because atom literals carry closed-shape domain information.
pub(super) fn widen_type_var_binding(value: Type) -> Type {
    match value {
        Type::LiteralInt(_) => Type::Int,
        other => other,
    }
}

/// Checks whether a type variable occurs inside a candidate binding.
///
/// Inputs:
/// - `var`: variable id being tested.
/// - `value`: candidate type value.
/// - `subst`: current substitutions to apply before traversal.
///
/// Output:
/// - `true` when binding would create a recursive type.
///
/// Transformation:
/// - Applies substitutions and recursively scans composite type children.
pub(super) fn occurs(var: TypeVarId, value: &Type, subst: &HashMap<TypeVarId, Type>) -> bool {
    match apply_subst(value, subst) {
        Type::Var(other) => other == var,
        Type::Apply { constructor, args } => {
            constructor == var || args.iter().any(|arg| occurs(var, arg, subst))
        }
        Type::Existential { params, body } => {
            !params.contains(&var) && occurs(var, body.as_ref(), subst)
        }
        Type::List(inner) => occurs(var, &inner, subst),
        Type::Tuple(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Union(items) => items.iter().any(|item| occurs(var, item, subst)),
        Type::Named { args, .. } => args.iter().any(|arg| occurs(var, arg, subst)),
        Type::Map(fields) => fields.iter().any(|field| occurs(var, &field.value, subst)),
        Type::Function { params, ret } => {
            params.iter().any(|param| occurs(var, param, subst)) || occurs(var, &ret, subst)
        }
        _ => false,
    }
}

/// Reveals opaque alias bodies for internal compatibility checks.
///
/// Inputs:
/// - `ty`: type tree that may reference opaque aliases.
/// - `aliases`: known type aliases.
///
/// Output:
/// - Type tree with directly referenced local opaque aliases substituted.
///
/// Transformation:
/// - Replaces matching opaque aliases with parameter-substituted bodies and
///   recursively processes composite type children.
pub(super) fn reveal_opaque_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque && alias.params.len() == args.len() {
                    let mapping = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect::<HashMap<_, _>>();
                    return substitute_type_vars(&alias.body, &mapping);
                }
            }
            Type::Named {
                module: None,
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| reveal_opaque_aliases(arg, aliases))
                    .collect(),
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases(arg, aliases))
                .collect(),
        },
        Type::Apply { constructor, args } => Type::Apply {
            constructor: *constructor,
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases(arg, aliases))
                .collect(),
        },
        Type::Existential { params, body } => Type::Existential {
            params: params.clone(),
            body: Box::new(reveal_opaque_aliases(body, aliases)),
        },
        Type::List(inner) => Type::List(Box::new(reveal_opaque_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| reveal_opaque_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: reveal_opaque_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| reveal_opaque_aliases(param, aliases))
                .collect(),
            ret: Box::new(reveal_opaque_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(reveal_opaque_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

/// Applies type-variable substitutions to a type tree.
///
/// Inputs:
/// - `ty`: type that may contain variables.
/// - `subst`: variable substitutions produced during unification.
///
/// Output:
/// - Type tree with all reachable substitutions applied.
///
/// Transformation:
/// - Recursively follows variable bindings and rewrites composite type children.
pub(super) fn apply_subst(ty: &Type, subst: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => match subst.get(id) {
            Some(inner) => apply_subst(inner, subst),
            None => Type::Var(*id),
        },
        Type::Apply { constructor, args } => {
            apply_type_constructor_subst(*constructor, args, subst)
        }
        Type::Existential { params, body } => {
            let scoped_subst = mapping_without_bound_params(subst, params);
            Type::Existential {
                params: params.clone(),
                body: Box::new(apply_subst(body, &scoped_subst)),
            }
        }
        Type::List(inner) => Type::List(Box::new(apply_subst(inner, subst))),
        Type::Tuple(items) => {
            Type::Tuple(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Union(items) => {
            Type::Union(items.iter().map(|item| apply_subst(item, subst)).collect())
        }
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: apply_subst(&field.value, subst),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|arg| apply_subst(arg, subst)).collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| apply_subst(param, subst))
                .collect(),
            ret: Box::new(apply_subst(ret, subst)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(apply_subst(elem, subst)),
        },
        other => other.clone(),
    }
}

/// Applies inference substitutions to a higher-kinded constructor application.
///
/// Inputs:
/// - `constructor`: type variable id used as an applied type constructor.
/// - `args`: applied type arguments.
/// - `subst`: inference substitution table produced by unification.
///
/// Output:
/// - A concrete named type when the constructor variable has been inferred as
///   a named type constructor.
/// - A still-higher-kinded application when the constructor remains a type
///   variable.
///
/// Transformation:
/// - Mirrors `substitute_type_constructor_application` for inference-time
///   substitutions so `F[A]` and values of type `Option[A]` can unify through
///   ordinary trait dispatch and receiver checking.
fn apply_type_constructor_subst(
    constructor: TypeVarId,
    args: &[Type],
    subst: &HashMap<TypeVarId, Type>,
) -> Type {
    let args = args
        .iter()
        .map(|arg| apply_subst(arg, subst))
        .collect::<Vec<_>>();

    match subst.get(&constructor) {
        Some(Type::Named {
            module,
            name,
            args: constructor_args,
        }) => {
            let mut applied_args = constructor_args
                .iter()
                .map(|arg| apply_subst(arg, subst))
                .collect::<Vec<_>>();
            applied_args.extend(args);
            Type::Named {
                module: module.clone(),
                name: name.clone(),
                args: applied_args,
            }
        }
        Some(Type::Var(next_constructor)) => Type::Apply {
            constructor: *next_constructor,
            args,
        },
        _ => Type::Apply { constructor, args },
    }
}
