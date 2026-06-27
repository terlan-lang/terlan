use std::collections::HashMap;

use crate::terlan_typeck::{MapFieldType, Type, TypeAlias, TypeVarId};

/// Expands visible non-opaque type aliases inside a type expression.
///
/// Inputs:
/// - `ty`: type expression that may reference local aliases.
/// - `aliases`: alias table keyed by source-level type name.
///
/// Output:
/// - Type expression with eligible aliases substituted recursively.
///
/// Transformation:
/// - Leaves opaque aliases named, substitutes non-opaque aliases with matching
///   generic arity, and recursively expands nested arguments and structural
///   type members.
pub(crate) fn expand_type_aliases(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> Type {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: None,
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: None,
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::Named {
            module: Some(module),
            name,
            args,
        } => {
            let qualified_name = format!("{}.{}", module, name);
            if let Some(alias) = aliases.get(&qualified_name) {
                if alias.is_opaque {
                    return Type::Named {
                        module: Some(module.clone()),
                        name: name.clone(),
                        args: args
                            .iter()
                            .map(|arg| expand_type_aliases(arg, aliases))
                            .collect(),
                    };
                }
                if alias.params.len() != args.len() {
                    return ty.clone();
                }
                let args = args
                    .iter()
                    .map(|arg| expand_type_aliases(arg, aliases))
                    .collect::<Vec<_>>();
                let mapping = alias
                    .params
                    .iter()
                    .cloned()
                    .zip(args)
                    .collect::<HashMap<_, _>>();
                expand_type_aliases(&substitute_type_vars(&alias.body, &mapping), aliases)
            } else {
                Type::Named {
                    module: Some(module.clone()),
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| expand_type_aliases(arg, aliases))
                        .collect(),
                }
            }
        }
        Type::Apply { constructor, args } => Type::Apply {
            constructor: *constructor,
            args: args
                .iter()
                .map(|arg| expand_type_aliases(arg, aliases))
                .collect(),
        },
        Type::Existential { params, body } => Type::Existential {
            params: params.clone(),
            body: Box::new(expand_type_aliases(body, aliases)),
        },
        Type::List(inner) => Type::List(Box::new(expand_type_aliases(inner, aliases))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: expand_type_aliases(&field.value, aliases),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| expand_type_aliases(item, aliases))
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| expand_type_aliases(param, aliases))
                .collect(),
            ret: Box::new(expand_type_aliases(ret, aliases)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(expand_type_aliases(elem, aliases)),
        },
        other => other.clone(),
    }
}

/// Substitutes type variables according to a concrete mapping.
///
/// Inputs:
/// - `ty`: type tree that may contain variables.
/// - `mapping`: variable-id to replacement-type mapping.
///
/// Output:
/// - A cloned type tree with mapped variables replaced.
///
/// Transformation:
/// - Recursively walks all composite type forms and leaves unmapped variables
///   unchanged.
pub(crate) fn substitute_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(id) => mapping.get(id).cloned().unwrap_or(Type::Var(*id)),
        Type::Apply { constructor, args } => {
            substitute_type_constructor_application(*constructor, args, mapping)
        }
        Type::Existential { params, body } => {
            let scoped_mapping = mapping_without_bound_params(mapping, params);
            Type::Existential {
                params: params.clone(),
                body: Box::new(substitute_type_vars(body, &scoped_mapping)),
            }
        }
        Type::List(inner) => Type::List(Box::new(substitute_type_vars(inner, mapping))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| substitute_type_vars(item, mapping))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: substitute_type_vars(&field.value, mapping),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_type_vars(arg, mapping))
                .collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| substitute_type_vars(param, mapping))
                .collect(),
            ret: Box::new(substitute_type_vars(ret, mapping)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(substitute_type_vars(elem, mapping)),
        },
        other => other.clone(),
    }
}

/// Removes substitutions that target variables bound by an existential type.
///
/// Inputs:
/// - `mapping`: outer substitution table.
/// - `bound_params`: existential parameter ids that introduce a nested scope.
///
/// Output:
/// - A substitution table with bound variables removed.
///
/// Transformation:
/// - Clones only entries whose variable id is not shadowed by the existential
///   binder, preventing outer inference from rewriting package internals.
pub(crate) fn mapping_without_bound_params(
    mapping: &HashMap<TypeVarId, Type>,
    bound_params: &[TypeVarId],
) -> HashMap<TypeVarId, Type> {
    mapping
        .iter()
        .filter(|(id, _)| !bound_params.contains(id))
        .map(|(id, ty)| (*id, ty.clone()))
        .collect()
}

/// Reports whether two existential types are alpha-equivalent.
///
/// Inputs:
/// - `lhs` and `rhs`: candidate type expressions.
///
/// Output:
/// - `true` only when both are existential packages with the same binder count
///   and structurally equal bodies modulo binder renaming.
///
/// Transformation:
/// - Delegates to binder-aware body comparison and rejects non-existential
///   shapes without expanding or unpacking them.
pub(crate) fn existential_types_are_alpha_equivalent(lhs: &Type, rhs: &Type) -> bool {
    match (lhs, rhs) {
        (
            Type::Existential {
                params: left_params,
                body: left_body,
            },
            Type::Existential {
                params: right_params,
                body: right_body,
            },
        ) => {
            existential_parts_are_alpha_equivalent(left_params, left_body, right_params, right_body)
        }
        _ => false,
    }
}

/// Compares existential bodies after renaming right binders to left binders.
///
/// Inputs:
/// - `left_params` and `right_params`: binder ids from both packages.
/// - `left_body` and `right_body`: packaged body types.
///
/// Output:
/// - `true` when bodies are structurally equal after alpha-renaming.
///
/// Transformation:
/// - Builds a right-to-left binder map and rewrites the right body before
///   comparing it to the left body.
fn existential_parts_are_alpha_equivalent(
    left_params: &[TypeVarId],
    left_body: &Type,
    right_params: &[TypeVarId],
    right_body: &Type,
) -> bool {
    if left_params.len() != right_params.len() {
        return false;
    }
    let mapping = right_params
        .iter()
        .copied()
        .zip(left_params.iter().copied())
        .collect::<HashMap<_, _>>();
    rename_type_vars(right_body, &mapping) == *left_body
}

/// Renames type-variable ids inside a type tree.
///
/// Inputs:
/// - `ty`: type tree to rewrite.
/// - `mapping`: source variable id to target variable id.
///
/// Output:
/// - A cloned type tree with matching variable ids renamed.
///
/// Transformation:
/// - Recursively rewrites variable and higher-kinded constructor ids while
///   preserving every non-variable type shape.
fn rename_type_vars(ty: &Type, mapping: &HashMap<TypeVarId, TypeVarId>) -> Type {
    match ty {
        Type::Var(id) => Type::Var(mapping.get(id).copied().unwrap_or(*id)),
        Type::Apply { constructor, args } => Type::Apply {
            constructor: mapping.get(constructor).copied().unwrap_or(*constructor),
            args: args
                .iter()
                .map(|arg| rename_type_vars(arg, mapping))
                .collect(),
        },
        Type::Existential { params, body } => Type::Existential {
            params: params
                .iter()
                .map(|param| mapping.get(param).copied().unwrap_or(*param))
                .collect(),
            body: Box::new(rename_type_vars(body, mapping)),
        },
        Type::List(inner) => Type::List(Box::new(rename_type_vars(inner, mapping))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| rename_type_vars(item, mapping))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| rename_type_vars(item, mapping))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: rename_type_vars(&field.value, mapping),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| rename_type_vars(arg, mapping))
                .collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| rename_type_vars(param, mapping))
                .collect(),
            ret: Box::new(rename_type_vars(ret, mapping)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(rename_type_vars(elem, mapping)),
        },
        other => other.clone(),
    }
}

/// Substitutes a higher-kinded type-constructor application.
///
/// Inputs:
/// - `constructor`: type variable id used as the higher-kinded constructor.
/// - `args`: type arguments applied to that constructor.
/// - `mapping`: explicit type-variable replacement table.
///
/// Output:
/// - A named application such as `Option[T]` when the constructor variable is
///   mapped to a named type constructor.
/// - A rewritten `Type::Apply` when the constructor is unmapped or still maps
///   to another variable.
///
/// Transformation:
/// - Substitutes all argument types first, then rewrites `F[A]` through
///   mappings like `F = Option` so trait signatures can specialize
///   higher-kinded parameters without leaking internal constructor variables.
fn substitute_type_constructor_application(
    constructor: TypeVarId,
    args: &[Type],
    mapping: &HashMap<TypeVarId, Type>,
) -> Type {
    let args = args
        .iter()
        .map(|arg| substitute_type_vars(arg, mapping))
        .collect::<Vec<_>>();

    match mapping.get(&constructor) {
        Some(Type::Named {
            module,
            name,
            args: constructor_args,
        }) => {
            let mut applied_args = constructor_args
                .iter()
                .map(|arg| substitute_type_vars(arg, mapping))
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
