use std::collections::HashMap;

use terlan_syntax::{SyntaxPatternKind, SyntaxPatternOutput};

use crate::field_visibility::{split_private_field_spelling, struct_field_visibility_error};

use super::{
    alias_constructor_schemes, apply_subst, bind_var, expand_type_aliases,
    instantiate_constructor_scheme, is_literal_atom, is_map_type, next_constructor_type_var,
    normalize_union, parse_interface_constructor_schemes, pretty_type, unify, ConstructorScheme,
    ExprInferContext, MapFieldType, Type, TypeAlias, TypeVarId,
};

/// Checks record-pattern field visibility against expression context metadata.
///
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
    match (pattern.kind, variant) {
        (SyntaxPatternKind::Wildcard, _)
        | (SyntaxPatternKind::Var, _)
        | (SyntaxPatternKind::Ignore, _)
        | (SyntaxPatternKind::Placeholder, _) => true,
        (SyntaxPatternKind::Int, Type::Int | Type::Union(_)) => true,
        (SyntaxPatternKind::Float, Type::Float | Type::Union(_)) => true,
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
            name == head
                && pattern.children.len() == variant_items.len().saturating_sub(1)
                && pattern
                    .children
                    .iter()
                    .zip(variant_items.iter().skip(1))
                    .all(|(p, t)| syntax_pattern_subsumes_variant(p, t, aliases))
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

    match pattern.kind {
        SyntaxPatternKind::Var => {
            locals.insert(
                pattern.text.clone().unwrap_or_default(),
                apply_subst(&expected, subst),
            );
            Ok(())
        }
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Ok(()),
        SyntaxPatternKind::Int => unify(&expected, &Type::Int, subst),
        SyntaxPatternKind::Float => unify(&expected, &Type::Float, subst),
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

/// Checks a constructor pattern against a structural constructor shape.
///
/// Inputs:
/// - `pattern`: constructor-style pattern from syntax output.
/// - `expected`: expected scrutinee type.
/// - `aliases`: visible type aliases.
/// - `ctx`: optional expression inference context.
/// - `locals`: branch-local bindings to populate.
/// - `subst`: active type-variable substitutions.
///
/// Output:
/// - `Some(Ok(()))` when the structural shape matches, `Some(Err(_))` for an
///   applicable mismatch, or `None` when this helper cannot handle the shape.
///
/// Transformation:
/// - Matches constructor names to tuple atom heads or literal atoms and checks
///   child patterns against payload positions.
fn check_structural_constructor_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    ctx: Option<&ExprInferContext<'_>>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Result<(), String>> {
    let name = pattern.text.as_deref().unwrap_or_default();
    let atom = constructor_pattern_atom_name(name);
    match expected {
        Type::Union(variants) => {
            let mut last_error = None;
            for variant in variants {
                let mut trial_subst = subst.clone();
                let mut trial_locals = locals.clone();
                match check_structural_constructor_pattern(
                    pattern,
                    variant,
                    aliases,
                    ctx,
                    &mut trial_locals,
                    &mut trial_subst,
                ) {
                    Some(Ok(())) => {
                        *subst = trial_subst;
                        *locals = trial_locals;
                        return Some(Ok(()));
                    }
                    Some(Err(message)) => last_error = Some(message),
                    None => {}
                }
            }
            last_error.map(Err)
        }
        Type::Tuple(items) => {
            let Some(Type::LiteralAtom(head)) = items.first() else {
                return None;
            };
            if head != &atom {
                return None;
            }
            if pattern.children.len() != items.len().saturating_sub(1) {
                let expected_arity = items.len().saturating_sub(1);
                return Some(Err(format!(
                    "constructor {} has arity mismatch: expected {}..{} args, found {}",
                    name,
                    expected_arity,
                    expected_arity,
                    pattern.children.len()
                )));
            }
            for (child, expected_child) in pattern.children.iter().zip(items.iter().skip(1)) {
                if let Err(message) =
                    check_syntax_pattern(child, expected_child, aliases, ctx, locals, subst)
                {
                    return Some(Err(message));
                }
            }
            Some(Ok(()))
        }
        Type::LiteralAtom(head) if head == &atom && pattern.children.is_empty() => Some(Ok(())),
        _ => None,
    }
}

/// Converts a constructor-pattern name to its structural atom head.
///
/// Inputs:
/// - `name`: constructor-pattern identifier such as `Some`.
///
/// Output:
/// - Lowercase atom name used by structural tuple aliases.
///
/// Transformation:
/// - Lowercases all characters without adding punctuation or backend syntax.
fn constructor_pattern_atom_name(name: &str) -> String {
    name.chars()
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>()
}

/// Checks a constructor-style pattern against an expected type.
///
/// Inputs:
/// - `pattern`: constructor pattern.
/// - `expected`: type expected by the matched expression.
/// - `aliases`: aliases available for structural expansion.
/// - `ctx`: expression context used for constructor lookup.
/// - `locals`: mutable binding environment updated by successful bindings.
/// - `subst`: mutable type-variable substitution map.
///
/// Output:
/// - `Some(Ok(()))` when a constructor name is recognized and compatible.
/// - `Some(Err(_))` when the constructor name is recognized but invalid.
/// - `None` when the pattern is not constructor-shaped.
///
/// Transformation:
/// - Resolves local, alias, or imported constructor schemes, checks arity and
///   return compatibility, then recursively validates constructor payload
///   patterns against the selected scheme.
fn check_syntax_constructor_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    ctx: Option<&ExprInferContext<'_>>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Option<Result<(), String>> {
    let name = pattern.text.as_deref().unwrap_or_default();
    if !is_constructor_pattern_name(name) {
        return None;
    }
    let ctx = ctx?;
    if let Some(message) = imported_opaque_constructor_pattern_error(name, ctx) {
        return Some(Err(message));
    }
    let Some(schemes) = constructor_pattern_schemes(name, ctx) else {
        return Some(Err(format!("unknown constructor pattern {}", name)));
    };
    let mut last_error = None;

    for scheme in schemes {
        let instantiated = instantiate_constructor_scheme(
            &scheme,
            next_constructor_type_var(std::slice::from_ref(expected), subst),
        );
        let mut trial_subst = subst.clone();
        let mut trial_locals = locals.clone();

        let arity_ok = if instantiated.vararg.is_some() {
            pattern.children.len() >= instantiated.min_arity
        } else {
            pattern.children.len() >= instantiated.min_arity
                && pattern.children.len() <= instantiated.fixed_params.len()
        };
        if !arity_ok {
            last_error = Some(format!(
                "constructor {} has arity mismatch: expected {}..{} args, found {}",
                name,
                instantiated.min_arity,
                instantiated.fixed_params.len(),
                pattern.children.len()
            ));
            continue;
        }

        if let Err(message) =
            unify_constructor_pattern_return(expected, &instantiated.ret, &mut trial_subst)
        {
            last_error = Some(message);
            continue;
        }

        let mut failed = None;
        for (index, arg) in pattern.children.iter().enumerate() {
            let expected_arg = instantiated
                .fixed_params
                .get(index)
                .or(instantiated.vararg.as_ref())
                .cloned()
                .unwrap_or(Type::Dynamic);
            if let Err(message) = check_syntax_pattern(
                arg,
                &expected_arg,
                aliases,
                Some(ctx),
                &mut trial_locals,
                &mut trial_subst,
            ) {
                failed = Some(message);
                break;
            }
        }

        if let Some(message) = failed {
            last_error = Some(message);
            continue;
        }

        *subst = trial_subst;
        *locals = trial_locals;
        return Some(Ok(()));
    }

    Some(Err(last_error.unwrap_or_else(|| {
        format!(
            "no matching constructor {} / {}",
            name,
            pattern.children.len()
        )
    })))
}

/// Unifies a constructor-pattern return with the expected match type.
///
/// Inputs:
/// - `expected`: type expected by the matched expression.
/// - `actual`: constructor return type.
/// - `subst`: mutable type-variable substitution map.
///
/// Output:
/// - `Ok(())` when the constructor can inhabit the expected type.
///
/// Transformation:
/// - Tries each union variant independently when matching union types,
///   otherwise performs ordinary unification.
fn unify_constructor_pattern_return(
    expected: &Type,
    actual: &Type,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    match expected {
        Type::Union(variants) => {
            let mut last_error = None;
            for variant in variants {
                let mut trial = subst.clone();
                match unify(actual, variant, &mut trial) {
                    Ok(()) => {
                        *subst = trial;
                        return Ok(());
                    }
                    Err(message) => last_error = Some(message),
                }
            }
            Err(last_error.unwrap_or_else(|| {
                format!(
                    "expected {} found {}",
                    pretty_type(expected),
                    pretty_type(actual)
                )
            }))
        }
        _ => unify(actual, expected, subst),
    }
}

/// Resolves constructor schemes available to a pattern name.
///
/// Inputs:
/// - `name`: constructor-pattern name.
/// - `ctx`: expression inference context with local and imported constructors.
///
/// Output:
/// - Matching constructor schemes when the name resolves.
///
/// Transformation:
/// - Looks for explicit constructors, alias-generated constructors, and
///   imported interface constructors in that order.
fn constructor_pattern_schemes(
    name: &str,
    ctx: &ExprInferContext<'_>,
) -> Option<Vec<ConstructorScheme>> {
    if let Some(schemes) = ctx.constructors.get(name) {
        return Some(schemes.clone());
    }

    if let Some(schemes) = alias_constructor_schemes(name, ctx.aliases) {
        return Some(schemes);
    }

    let imported = ctx.constructor_aliases.get(name)?;
    let interface = ctx.interface_map.get(&imported.module)?;
    parse_interface_constructor_schemes(
        interface
            .constructors
            .get(&imported.name)
            .map(Vec::as_slice),
        interface,
    )
}

/// Returns the opaque-import error for an imported constructor pattern.
///
/// Inputs:
/// - `name`: constructor-pattern name.
/// - `ctx`: expression inference context with imported constructor aliases.
///
/// Output:
/// - Error message when the imported type is opaque outside its module.
///
/// Transformation:
/// - Resolves the constructor alias to its interface and rejects constructor
///   matching when the provider marks the type opaque.
fn imported_opaque_constructor_pattern_error(
    name: &str,
    ctx: &ExprInferContext<'_>,
) -> Option<String> {
    let imported = ctx.constructor_aliases.get(name)?;
    let interface = ctx.interface_map.get(&imported.module)?;
    interface.opaque_types.contains(&imported.name).then(|| {
        format!(
            "cannot match opaque type {}.{} as constructor pattern outside defining module",
            imported.module, imported.name
        )
    })
}

/// Returns whether a name is syntactically constructor-pattern shaped.
///
/// Inputs:
/// - `name`: candidate pattern name.
///
/// Output:
/// - `true` when the name starts with an uppercase ASCII character.
///
/// Transformation:
/// - Applies Terlan's constructor-pattern naming convention.
fn is_constructor_pattern_name(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}
