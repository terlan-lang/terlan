use std::collections::HashMap;

use crate::terlan_syntax::{SyntaxPatternKind, SyntaxPatternOutput};

use crate::terlan_typeck::field_visibility::{
    split_private_field_spelling, struct_field_visibility_error,
};

use super::{
    alias_constructor_schemes, apply_subst, bind_var, expand_type_aliases,
    instantiate_constructor_scheme, is_literal_atom, is_map_type, next_constructor_type_var,
    normalize_union, parse_interface_constructor_schemes, parse_type_expr, pretty_type,
    split_module_name, unify, ConstructorScheme, ExprInferContext, MapFieldType, Type, TypeAlias,
    TypeVarId,
};

mod binary_layout;
mod constructors;
use constructors::{
    check_structural_constructor_pattern, check_syntax_constructor_pattern,
    constructor_pattern_atom_name,
};
mod finite_coverage;
pub(super) use finite_coverage::{has_finite_fields, subtract_finite_pattern};
mod string_capture;
use binary_layout::check_binary_layout_pattern;
use string_capture::check_string_capture_pattern;

/// Checks record-pattern field visibility against expression context metadata.
/// Inputs:
/// - `struct_name`: expected struct type name for the record pattern.
/// - `field_key`: pattern field key, optionally written as `#field`.
/// - `ctx`: optional expression inference context with visibility/import data.
///
/// Output:
/// - `Ok(())` when the field key is visibility-compatible, otherwise a
///   diagnostic message.
///
/// Transformation:
/// - Normalizes private field spelling and delegates the actual visibility rule
///   to the shared typechecker helper.
fn check_record_pattern_field_visibility(
    struct_name: &str,
    field_key: &str,
    ctx: Option<&ExprInferContext<'_>>,
) -> Result<(), String> {
    let Some(ctx) = ctx else {
        return Ok(());
    };
    let (field_name, requested_private) = split_private_field_spelling(field_key);
    if let Some(message) = struct_field_visibility_error(
        struct_name,
        field_name,
        requested_private,
        ctx.struct_field_visibility,
        ctx.imported_type_names,
    ) {
        Err(message)
    } else {
        Ok(())
    }
}

/// Returns whether a pattern covers one variant of an expected union.
///
/// Inputs:
/// - `pattern`: syntax-output pattern to test.
/// - `variant`: one possible expected type variant.
/// - `aliases`: local and imported aliases used for expansion.
///
/// Output:
/// - `true` when the pattern structurally subsumes the variant.
///
/// Transformation:
/// - Expands aliases and compares wildcard, literal, constructor, tuple, list,
///   and map pattern shapes recursively.
pub(super) fn syntax_pattern_subsumes_variant(
    pattern: &SyntaxPatternOutput,
    variant: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let variant = expand_type_aliases(variant, aliases);
    if pattern.kind == SyntaxPatternKind::Constructor
        && pattern
            .text
            .as_deref()
            .is_some_and(|name| name.starts_with("$const:"))
    {
        return pattern
            .children
            .first()
            .is_some_and(|child| syntax_pattern_subsumes_variant(child, &variant, aliases));
    }
    if pattern.kind == SyntaxPatternKind::Constructor
        && constructor_pattern_alias_subsumes_variant(pattern, &variant, aliases)
    {
        return true;
    }
    match (pattern.kind, variant) {
        (SyntaxPatternKind::Wildcard, _)
        | (SyntaxPatternKind::Var, _)
        | (SyntaxPatternKind::Alias, _)
        | (SyntaxPatternKind::Ignore, _)
        | (SyntaxPatternKind::Placeholder, _) => true,
        (SyntaxPatternKind::Int, Type::Int | Type::Union(_)) => true,
        (SyntaxPatternKind::Float, Type::Float | Type::Union(_)) => true,
        (
            SyntaxPatternKind::String | SyntaxPatternKind::StringPattern,
            Type::Binary | Type::Union(_),
        ) => true,
        (SyntaxPatternKind::Atom, Type::LiteralAtom(b)) => {
            pattern.text.as_deref().is_some_and(|a| a == b.as_str())
        }
        (SyntaxPatternKind::Atom, Type::Atom) => true,
        (SyntaxPatternKind::Constructor, Type::Tuple(variant_items)) => {
            let Some(Type::LiteralAtom(head)) = variant_items.first() else {
                return false;
            };
            let Some(name) = pattern.text.as_deref() else {
                return false;
            };
            constructor_pattern_atom_name(name) == *head
                && pattern.children.len() == variant_items.len().saturating_sub(1)
                && pattern
                    .children
                    .iter()
                    .zip(variant_items.iter().skip(1))
                    .all(|(p, t)| syntax_pattern_subsumes_variant(p, t, aliases))
        }
        (SyntaxPatternKind::Constructor, Type::LiteralAtom(head)) => {
            pattern.text.as_deref().is_some_and(|name| {
                pattern.children.is_empty() && constructor_pattern_atom_name(name) == head
            })
        }
        (SyntaxPatternKind::Tuple, Type::Tuple(variant_items)) => {
            if pattern.children.len() != variant_items.len() {
                return false;
            }
            pattern
                .children
                .iter()
                .zip(variant_items.iter())
                .all(|(p, t)| syntax_pattern_subsumes_variant(p, t, aliases))
        }
        (SyntaxPatternKind::List, Type::List(_)) => true,
        (SyntaxPatternKind::ListCons, Type::List(_)) => true,
        (SyntaxPatternKind::Map, ty) => match ty {
            Type::Map(map_type) => pattern
                .fields
                .iter()
                .all(|field| map_type.iter().any(|entry| entry.key == field.key)),
            _ => is_map_type(&ty, aliases),
        },
        (SyntaxPatternKind::MapField, ty) => is_map_type(&ty, aliases),
        (_, Type::Union(variants)) => variants
            .iter()
            .any(|v| syntax_pattern_subsumes_variant(pattern, v, aliases)),
        _ => false,
    }
}

fn constructor_pattern_alias_subsumes_variant(
    pattern: &SyntaxPatternOutput,
    variant: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let Some(name) = pattern.text.as_deref() else {
        return false;
    };
    let Some(schemes) = alias_constructor_schemes(name, aliases) else {
        return false;
    };

    schemes.into_iter().any(|scheme| {
        if pattern.children.len() != scheme.fixed_params.len() {
            return false;
        }

        let mut subst = HashMap::new();
        if unify(&scheme.ret, variant, &mut subst).is_err() {
            return false;
        }

        pattern
            .children
            .iter()
            .zip(scheme.fixed_params.iter())
            .all(|(child, param)| {
                let expected = apply_subst(param, &subst);
                syntax_pattern_subsumes_variant(child, &expected, aliases)
            })
    })
}

/// Flattens a type into the variants relevant for exhaustiveness checks.
///
/// Inputs:
/// - `ty`: expected type being matched.
///
/// Output:
/// - Normalized, flattened variant list.
///
/// Transformation:
/// - Expands aliases with an empty alias environment, normalizes unions,
///   flattens nested unions, and drops `Never`.
pub(super) fn as_exhaustive_union_variants(ty: &Type) -> Vec<Type> {
    match normalize_union(vec![expand_type_aliases(ty, &HashMap::new())]) {
        Type::Union(items) => {
            let mut out = Vec::new();
            for item in items {
                match item {
                    Type::Union(nested) => {
                        out.extend(nested);
                    }
                    other => out.push(other),
                }
            }
            out
        }
        Type::Never => Vec::new(),
        other => vec![other],
    }
}

/// Builds the broad type shape required by a structural pattern.
///
/// Inputs:
/// - `pattern`: syntax-output pattern being checked against an unconstrained
///   generic type variable.
///
/// Output:
/// - A broad `Type` that represents the minimum structural shape required by
///   the pattern.
///
/// Transformation:
/// - Preserves structural containers such as tuples, lists, and maps while
///   assigning `Dynamic` to value-binding leaves. Literal leaves keep their
///   primitive type so generic pattern payloads can be constrained without
///   inventing constructor-specific rules.
fn syntax_pattern_shape_type(pattern: &SyntaxPatternOutput) -> Type {
    match pattern.kind {
        SyntaxPatternKind::Int => Type::Int,
        SyntaxPatternKind::Float => Type::Float,
        SyntaxPatternKind::String | SyntaxPatternKind::StringPattern => Type::Binary,
        SyntaxPatternKind::Atom => {
            let atom = pattern.text.as_deref().unwrap_or_default();
            if atom == "true" || atom == "false" {
                Type::Bool
            } else if is_literal_atom(atom) {
                Type::LiteralAtom(atom.to_string())
            } else {
                Type::Atom
            }
        }
        SyntaxPatternKind::Tuple => Type::Tuple(
            pattern
                .children
                .iter()
                .map(syntax_pattern_shape_type)
                .collect(),
        ),
        SyntaxPatternKind::Alias => pattern
            .children
            .first()
            .map(syntax_pattern_shape_type)
            .unwrap_or(Type::Dynamic),
        SyntaxPatternKind::List | SyntaxPatternKind::ListCons => Type::List(Box::new(
            pattern
                .children
                .first()
                .map(syntax_pattern_shape_type)
                .unwrap_or(Type::Dynamic),
        )),
        SyntaxPatternKind::Map | SyntaxPatternKind::MapField => Type::Map(
            pattern
                .fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: syntax_pattern_shape_type(&field.value),
                    required: true,
                })
                .collect(),
        ),
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Var
        | SyntaxPatternKind::StringCapture
        | SyntaxPatternKind::BinaryLayout
        | SyntaxPatternKind::Constructor
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder
        | SyntaxPatternKind::Record => Type::Dynamic,
    }
}

/// Checks a syntax pattern against an expected type.
///
/// Inputs:
/// - `pattern`: syntax-output pattern.
/// - `expected`: type expected by the matched expression.
/// - `aliases`: aliases available for structural expansion.
/// - `ctx`: optional expression context used for constructor-pattern lookup.
/// - `locals`: mutable binding environment updated by successful bindings.
/// - `subst`: mutable type-variable substitution map.
///
/// Output:
/// - `Ok(())` when the pattern is compatible, otherwise a diagnostic message.
///
/// Transformation:
/// - Expands aliases, recursively validates structural pattern children,
///   inserts pattern bindings after active substitutions are applied, and
///   applies constructor-pattern schemes when a constructor context is
///   available.
pub(super) fn check_syntax_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    ctx: Option<&ExprInferContext<'_>>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    let expected = apply_subst(&expand_type_aliases(expected, aliases), subst);
    if let Some(union) = pattern
        .text
        .as_deref()
        .and_then(|text| text.strip_prefix("$const:"))
        .and_then(|qualified| qualified.rsplit_once('.').map(|(union, _)| union))
    {
        let (module, name) = split_module_name(union);
        return unify(
            &expected,
            &Type::Named {
                module,
                name,
                args: Vec::new(),
            },
            subst,
        );
    }

    match pattern.kind {
        SyntaxPatternKind::Var => {
            locals.insert(
                pattern.text.clone().unwrap_or_default(),
                apply_subst(&expected, subst),
            );
            Ok(())
        }
        SyntaxPatternKind::Alias => {
            locals.insert(
                pattern.text.clone().unwrap_or_default(),
                apply_subst(&expected, subst),
            );
            let child = pattern
                .children
                .first()
                .ok_or_else(|| "alias pattern requires one child pattern".to_string())?;
            check_syntax_pattern(child, &expected, aliases, ctx, locals, subst)
        }
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Ok(()),
        SyntaxPatternKind::Int => unify(&expected, &Type::Int, subst),
        SyntaxPatternKind::Float => unify(&expected, &Type::Float, subst),
        SyntaxPatternKind::String => unify(&expected, &Type::Binary, subst),
        SyntaxPatternKind::StringPattern => {
            check_string_capture_pattern(pattern, &expected, aliases, locals, subst)
        }
        SyntaxPatternKind::StringCapture => {
            Err("string capture nodes must appear inside a string pattern".to_string())
        }
        SyntaxPatternKind::BinaryLayout => {
            check_binary_layout_pattern(pattern, &expected, locals, subst)
        }
        SyntaxPatternKind::Atom => {
            let atom = pattern.text.as_deref().unwrap_or_default();
            if atom.starts_with('_') {
                return Ok(());
            }
            if atom == "[]" || atom == "nil" {
                return match &expected {
                    Type::List(_) | Type::Dynamic | Type::Term => Ok(()),
                    _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst),
                };
            }
            if atom == "true" || atom == "false" {
                return unify(&expected, &Type::Bool, subst);
            }
            if is_literal_atom(atom) {
                unify(&expected, &Type::LiteralAtom(atom.to_string()), subst)
            } else {
                unify(&expected, &Type::Atom, subst)
            }
        }
        SyntaxPatternKind::Constructor => {
            if pattern
                .text
                .as_deref()
                .is_some_and(|name| name.starts_with("$const:"))
            {
                let child = pattern.children.first().ok_or_else(|| {
                    "constant value pattern requires its substituted literal".to_string()
                })?;
                return check_syntax_pattern(child, &expected, aliases, ctx, locals, subst);
            }
            if let Some(result) = check_structural_constructor_pattern(
                pattern, &expected, aliases, ctx, locals, subst,
            ) {
                return result;
            }
            check_syntax_constructor_pattern(pattern, &expected, aliases, ctx, locals, subst)
                .unwrap_or_else(|| {
                    Err(format!(
                        "expected {} found constructor pattern",
                        pretty_type(&expected)
                    ))
                })
        }
        SyntaxPatternKind::Tuple => match &expected {
            Type::Var(id) => {
                bind_var(*id, syntax_pattern_shape_type(pattern), subst)?;
                let specialized = apply_subst(&Type::Var(*id), subst);
                check_syntax_pattern(pattern, &specialized, aliases, ctx, locals, subst)
            }
            Type::Union(variants) => {
                let mut ok = false;
                for variant in variants {
                    let mut subst_before = subst.clone();
                    let mut locals_before = locals.clone();
                    if check_syntax_pattern(
                        pattern,
                        variant,
                        aliases,
                        ctx,
                        &mut locals_before,
                        &mut subst_before,
                    )
                    .is_ok()
                    {
                        *subst = subst_before;
                        for (name, value) in locals_before.into_iter() {
                            locals.insert(name, value);
                        }
                        ok = true;
                        break;
                    }
                }
                if ok {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found tuple pattern",
                        pretty_type(&expected)
                    ))
                }
            }
            Type::Tuple(variant_items) => {
                if variant_items.len() != pattern.children.len() {
                    return Err(format!(
                        "tuple arity mismatch: expected {} elements, found {}",
                        variant_items.len(),
                        pattern.children.len()
                    ));
                }
                for (pattern_item, expected_item) in
                    pattern.children.iter().zip(variant_items.iter())
                {
                    check_syntax_pattern(pattern_item, expected_item, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for pattern_item in &pattern.children {
                    check_syntax_pattern(
                        pattern_item,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found tuple pattern",
                pretty_type(&expected)
            )),
        },
        SyntaxPatternKind::List => match &expected {
            Type::List(elem) => {
                for item in &pattern.children {
                    check_syntax_pattern(item, elem, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for item in &pattern.children {
                    check_syntax_pattern(item, &Type::Dynamic, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst).map(|_| ()),
        },
        SyntaxPatternKind::ListCons => match &expected {
            Type::List(elem) => {
                if let Some(head) = pattern.children.first() {
                    check_syntax_pattern(head, elem, aliases, ctx, locals, subst)?;
                }
                if let Some(tail) = pattern.children.get(1) {
                    check_syntax_pattern(
                        tail,
                        &Type::List(elem.clone()),
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            Type::Dynamic | Type::Term => {
                for item in &pattern.children {
                    check_syntax_pattern(item, &Type::Dynamic, aliases, ctx, locals, subst)?;
                }
                Ok(())
            }
            _ => unify(&expected, &Type::List(Box::new(Type::Dynamic)), subst).map(|_| ()),
        },
        SyntaxPatternKind::Map => match &expected {
            Type::Map(expected_fields) => {
                for pattern_field in &pattern.fields {
                    match expected_fields
                        .iter()
                        .find(|field| field.key == pattern_field.key)
                    {
                        Some(field) => check_syntax_pattern(
                            &pattern_field.value,
                            &field.value,
                            aliases,
                            ctx,
                            locals,
                            subst,
                        )?,
                        None => {
                            return Err(format!("unknown map key {}", pattern_field.key));
                        }
                    };
                }
                Ok(())
            }
            _ if is_map_type(&expected, aliases) => {
                for pattern_field in &pattern.fields {
                    check_syntax_pattern(
                        &pattern_field.value,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found map pattern",
                pretty_type(&expected)
            )),
        },
        SyntaxPatternKind::MapField => {
            if is_map_type(&expected, aliases) {
                if let Some(value) = pattern.children.first() {
                    check_syntax_pattern(value, &Type::Dynamic, aliases, ctx, locals, subst)
                } else if let Some(field) = pattern.fields.first() {
                    check_syntax_pattern(&field.value, &Type::Dynamic, aliases, ctx, locals, subst)
                } else {
                    Ok(())
                }
            } else {
                Err(format!(
                    "expected {} found map pattern",
                    pretty_type(&expected)
                ))
            }
        }
        SyntaxPatternKind::Record => match &expected {
            Type::Union(variants) => {
                if variants.iter().any(|variant| {
                    check_syntax_pattern(pattern, variant, aliases, ctx, locals, subst).is_ok()
                }) {
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found record pattern {}",
                        pretty_type(&expected),
                        pattern.text.as_deref().unwrap_or_default()
                    ))
                }
            }
            Type::Named {
                module: _,
                name: expected_name,
                ..
            } => {
                let name = pattern.text.as_deref().unwrap_or_default();
                if expected_name == name {
                    for field in &pattern.fields {
                        check_record_pattern_field_visibility(expected_name, &field.key, ctx)?;
                        check_syntax_pattern(
                            &field.value,
                            &Type::Dynamic,
                            aliases,
                            ctx,
                            locals,
                            subst,
                        )?;
                    }
                    Ok(())
                } else {
                    Err(format!(
                        "expected {} found record pattern {}",
                        pretty_type(&expected),
                        name
                    ))
                }
            }
            _ if matches!(expected, Type::Dynamic | Type::Term) => {
                for field in &pattern.fields {
                    check_syntax_pattern(
                        &field.value,
                        &Type::Dynamic,
                        aliases,
                        ctx,
                        locals,
                        subst,
                    )?;
                }
                Ok(())
            }
            _ => Err(format!(
                "expected {} found record pattern {}",
                pretty_type(&expected),
                pattern.text.as_deref().unwrap_or_default()
            )),
        },
    }
}
