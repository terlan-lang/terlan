//! Constructor pattern resolution, structural matching, and instantiation.

use super::{
    alias_constructor_schemes, apply_subst, check_syntax_pattern, instantiate_constructor_scheme,
    next_constructor_type_var, parse_interface_constructor_schemes, pretty_type, unify,
    ConstructorScheme, ExprInferContext, HashMap, SyntaxPatternOutput, Type, TypeAlias, TypeVarId,
};

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
pub(super) fn check_structural_constructor_pattern(
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
pub(super) fn constructor_pattern_atom_name(name: &str) -> String {
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
pub(super) fn check_syntax_constructor_pattern(
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
            let expected_arg = apply_subst(&expected_arg, &trial_subst);
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
