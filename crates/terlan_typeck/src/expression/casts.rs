use super::*;

/// Infers the type for a syntax-output cast expression.
///
/// Inputs:
/// - `expr`: syntax-output cast node with one child and target type text.
/// - `locals`, `ctx`, and `subst`: the active expression inference context.
/// - `errors`: mutable diagnostic text sink for unsupported conversion claims.
///
/// Output:
/// - Parsed target type when available, otherwise `Dynamic`.
///
/// Transformation:
/// - Type-checks the cast source child, parses the preserved target type text,
///   accepts casts that are already compatible after substitutions and aliases,
///   and rejects conversions that still require explicit conversion-trait
///   resolution.
pub(super) fn infer_syntax_cast_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let source_type = expr
        .children
        .first()
        .map(|child| infer_syntax_expr(child, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    let target_text = expr.text.as_deref().unwrap_or("Dynamic");
    let mut vars = HashMap::new();
    let mut next_var = 0;
    let alias_names = ctx.aliases.keys().cloned().collect::<HashSet<_>>();
    let target_type = parse_type_expr(target_text, &alias_names, &mut vars, &mut next_var)
        .unwrap_or_else(|| {
            errors.push(format!("invalid cast target type `{}`", target_text));
            Type::Dynamic
        });

    if !cast_source_is_assignable_to_target(&source_type, &target_type, ctx, subst)
        && !cast_source_has_conversion_to_target(&source_type, &target_type, ctx, subst)
    {
        errors.push(format!(
            "cast from {} to {} requires trait-backed conversion resolution before backend emission",
            pretty_type(&apply_subst(&source_type, subst)),
            pretty_type(&target_type)
        ));
    }
    target_type
}

/// Returns whether a cast source can already be viewed as the target type.
///
/// Inputs:
/// - `source_type`: inferred source expression type.
/// - `target_type`: parsed cast target type.
/// - `ctx` and `subst`: active expression inference context and substitutions.
///
/// Output:
/// - `true` when no runtime or trait-backed conversion is required.
///
/// Transformation:
/// - Applies current substitutions, expands visible type aliases on both sides,
///   then delegates to the existing subtype relation so literal widening,
///   `Number`, `Term`, and Unit equivalence stay consistent with the rest of
///   typechecking.
fn cast_source_is_assignable_to_target(
    source_type: &Type,
    target_type: &Type,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let source = apply_subst(source_type, subst);
    let target = apply_subst(target_type, subst);
    is_subtype_with_aliases(&source, &target, ctx.aliases)
}

/// Returns whether a cast has an explicit conversion trait conformance.
///
/// Inputs:
/// - `source_type`: inferred source expression type.
/// - `target_type`: parsed cast target type.
/// - `ctx` and `subst`: active expression inference context and substitutions.
///
/// Output:
/// - `true` when a visible `Convertable[Source, Target]` implementation or
///   active generic bound proves the conversion is explicit.
///
/// Transformation:
/// - Applies substitutions, expands local aliases, canonicalizes the two trait
///   arguments, and reuses the normal trait-bound lookup cache so cast
///   conversion proof follows the same conformance visibility rules as generic
///   function bounds.
fn cast_source_has_conversion_to_target(
    source_type: &Type,
    target_type: &Type,
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let source = expand_type_aliases(&apply_subst(source_type, subst), ctx.aliases);
    let target = expand_type_aliases(&apply_subst(target_type, subst), ctx.aliases);
    let bound_args = canonicalize_trait_lookup_types(&[source, target]);
    trait_has_bound_implementation("Convertable", &bound_args, ctx)
}
