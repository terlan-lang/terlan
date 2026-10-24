//! Internal compatibility view of opaque alias bodies.

use super::{substitute_type_vars, MapFieldType, Type, TypeAlias};
use std::collections::{HashMap, HashSet};

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
pub(in super::super) fn reveal_opaque_aliases(
    ty: &Type,
    aliases: &HashMap<String, TypeAlias>,
    current_module: &str,
) -> Type {
    reveal_opaque_aliases_inner(ty, aliases, current_module, &mut HashSet::new())
}

fn reveal_opaque_aliases_inner(
    ty: &Type,
    aliases: &HashMap<String, TypeAlias>,
    current_module: &str,
    active: &mut HashSet<String>,
) -> Type {
    match ty {
        Type::Named { module, name, args }
            if module
                .as_deref()
                .is_none_or(|owner| owner == current_module) =>
        {
            if let Some(alias) = aliases.get(name) {
                if alias.is_opaque && alias.params.len() == args.len() && !active.contains(name) {
                    // Arguments are finite caller input, rather than recursive
                    // references introduced by this alias's representation.
                    let args = args
                        .iter()
                        .map(|arg| {
                            reveal_opaque_aliases_inner(arg, aliases, current_module, active)
                        })
                        .collect::<Vec<_>>();
                    active.insert(name.clone());
                    let mapping = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let body = substitute_type_vars(&alias.body, &mapping);
                    let revealed =
                        reveal_opaque_aliases_inner(&body, aliases, current_module, active);
                    active.remove(name);
                    return revealed;
                }
            }
            Type::Named {
                module: module.clone(),
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| reveal_opaque_aliases_inner(arg, aliases, current_module, active))
                    .collect(),
            }
        }
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases_inner(arg, aliases, current_module, active))
                .collect(),
        },
        Type::Apply { constructor, args } => Type::Apply {
            constructor: *constructor,
            args: args
                .iter()
                .map(|arg| reveal_opaque_aliases_inner(arg, aliases, current_module, active))
                .collect(),
        },
        Type::Existential { params, body } => Type::Existential {
            params: params.clone(),
            body: Box::new(reveal_opaque_aliases_inner(
                body,
                aliases,
                current_module,
                active,
            )),
        },
        Type::List(inner) => Type::List(Box::new(reveal_opaque_aliases_inner(
            inner,
            aliases,
            current_module,
            active,
        ))),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| reveal_opaque_aliases_inner(item, aliases, current_module, active))
                .collect(),
        ),
        Type::Union(items) => Type::Union(
            items
                .iter()
                .map(|item| reveal_opaque_aliases_inner(item, aliases, current_module, active))
                .collect(),
        ),
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: reveal_opaque_aliases_inner(
                        &field.value,
                        aliases,
                        current_module,
                        active,
                    ),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| reveal_opaque_aliases_inner(param, aliases, current_module, active))
                .collect(),
            ret: Box::new(reveal_opaque_aliases_inner(
                ret,
                aliases,
                current_module,
                active,
            )),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(reveal_opaque_aliases_inner(
                elem,
                aliases,
                current_module,
                active,
            )),
        },
        other => other.clone(),
    }
}
