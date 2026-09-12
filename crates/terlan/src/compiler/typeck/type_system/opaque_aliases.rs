//! Internal compatibility view of opaque alias bodies.

use super::{substitute_type_vars, MapFieldType, Type, TypeAlias};
use std::collections::HashMap;

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
) -> Type {
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
