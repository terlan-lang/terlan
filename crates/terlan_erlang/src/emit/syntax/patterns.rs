use super::*;

/// Lowers one syntax-output pattern into an Erlang pattern.
///
/// Inputs:
/// - `pattern`: parsed/formal syntax pattern emitted by `terlan_syntax`.
/// - `ctx`: syntax lowering context containing constructor and alias metadata.
///
/// Output:
/// - Erlang pattern model when the syntax pattern can be represented by the
///   current BEAM bridge.
/// - `None` for unsupported or malformed transitional syntax output.
///
/// Transformation:
/// - Converts literals, bindings, tuples, lists, maps, records, and declared
///   constructor-pattern sugar into Erlang pattern forms.
pub(super) fn lower_syntax_pattern(
    pattern: &SyntaxPatternOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    match pattern.kind {
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => Some(ErlPattern::Wildcard),
        SyntaxPatternKind::Var => {
            let name = pattern.text.as_deref()?;
            if is_bool_literal_name(name) {
                Some(ErlPattern::Atom(name.to_string()))
            } else {
                Some(ErlPattern::Var(sanitize_erlang_var(name)))
            }
        }
        SyntaxPatternKind::Int => Some(ErlPattern::Int(pattern.text.as_deref()?.parse().ok()?)),
        SyntaxPatternKind::Float => Some(ErlPattern::Float(pattern.text.clone()?)),
        SyntaxPatternKind::Atom => Some(ErlPattern::Atom(pattern.text.clone()?)),
        SyntaxPatternKind::Tuple => Some(ErlPattern::Tuple(
            pattern
                .children
                .iter()
                .map(|child| lower_syntax_pattern(child, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::List => Some(ErlPattern::List(
            pattern
                .children
                .iter()
                .map(|child| lower_syntax_pattern(child, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::ListCons => Some(ErlPattern::ListCons(
            Box::new(lower_syntax_pattern(pattern.children.first()?, ctx)?),
            Box::new(lower_syntax_pattern(pattern.children.get(1)?, ctx)?),
        )),
        SyntaxPatternKind::MapField => Some(ErlPattern::Map(vec![ErlPatternMapField {
            key: pattern.text.clone()?,
            value: lower_syntax_pattern(pattern.children.first()?, ctx)?,
            required: pattern.fields.first().is_none_or(|field| field.required),
        }])),
        SyntaxPatternKind::Map => Some(ErlPattern::Map(
            pattern
                .fields
                .iter()
                .map(|field| lower_syntax_pattern_field(field, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxPatternKind::Record => Some(ErlPattern::Record {
            name: pattern.text.clone()?,
            fields: pattern
                .fields
                .iter()
                .map(|field| lower_syntax_pattern_field(field, ctx))
                .collect::<Option<Vec<_>>>()?,
        }),
        SyntaxPatternKind::Constructor => {
            let name = pattern.text.as_deref()?;
            if let Some(target) = ctx.constructor_pattern_target(name, pattern.children.len()) {
                return lower_syntax_explicit_constructor_pattern(target, &pattern.children, ctx);
            }
            let target = ctx.alias_constructor_target(name, pattern.children.len())?;
            lower_syntax_constructor_pattern(target, &pattern.children, ctx)
        }
    }
}

/// Lowers one map or record pattern field.
///
/// Inputs:
/// - `field`: syntax-output field pattern.
/// - `ctx`: syntax lowering context for nested patterns.
///
/// Output:
/// - Erlang pattern map field with required/optional metadata preserved.
///
/// Transformation:
/// - Lowers the nested field value recursively while preserving the original
///   field key emitted by the syntax parser.
fn lower_syntax_pattern_field(
    field: &SyntaxPatternFieldOutput,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPatternMapField> {
    Some(ErlPatternMapField {
        key: field.key.clone(),
        value: lower_syntax_pattern(&field.value, ctx)?,
        required: field.required,
    })
}

/// Lowers an explicit constructor-pattern target.
///
/// Inputs:
/// - `target`: constructor pattern metadata produced from a constructor decl.
/// - `args`: source pattern arguments.
/// - `ctx`: syntax lowering context for nested patterns.
///
/// Output:
/// - Erlang pattern generated from the constructor body.
///
/// Transformation:
/// - Binds constructor parameter names to source argument patterns and lowers
///   the constructor body expression as a pattern template.
fn lower_syntax_explicit_constructor_pattern(
    target: &SyntaxConstructorPatternTarget,
    args: &[SyntaxPatternOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_constructor_pattern(&target.body, &bindings, ctx)
}

/// Lowers an alias constructor-pattern target.
///
/// Inputs:
/// - `target`: alias constructor metadata produced from a single-shape alias.
/// - `args`: source pattern arguments.
/// - `ctx`: syntax lowering context for nested patterns.
///
/// Output:
/// - Erlang pattern generated from the alias body.
///
/// Transformation:
/// - Binds alias parameter names to source argument patterns and lowers the
///   alias body expression as a pattern template.
fn lower_syntax_constructor_pattern(
    target: &SyntaxAliasConstructorTarget,
    args: &[SyntaxPatternOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_constructor_pattern(&target.body, &bindings, ctx)
}

/// Lowers constructor body syntax as a pattern template.
///
/// Inputs:
/// - `expr`: constructor or alias body expression.
/// - `bindings`: constructor parameter names mapped to caller patterns.
/// - `ctx`: syntax lowering context for nested bound patterns.
///
/// Output:
/// - Erlang pattern for the constructor body.
///
/// Transformation:
/// - Replaces body variables that match constructor parameters with caller
///   patterns while keeping literal tuple/list/list-cons structure intact.
fn syntax_expr_to_constructor_pattern(
    expr: &SyntaxExprOutput,
    bindings: &BTreeMap<String, &SyntaxPatternOutput>,
    ctx: &SyntaxLowerCtx,
) -> Option<ErlPattern> {
    match expr.kind {
        SyntaxExprKind::Atom => Some(ErlPattern::Atom(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            bindings
                .get(name)
                .and_then(|pattern| lower_syntax_pattern(pattern, ctx))
                .or_else(|| Some(ErlPattern::Var(sanitize_erlang_var(name))))
        }
        SyntaxExprKind::Int => Some(ErlPattern::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlPattern::Float(expr.text.clone()?)),
        SyntaxExprKind::Tuple => Some(ErlPattern::Tuple(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_constructor_pattern(item, bindings, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlPattern::List(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_constructor_pattern(item, bindings, ctx))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlPattern::ListCons(
            Box::new(syntax_expr_to_constructor_pattern(
                expr.children.first()?,
                bindings,
                ctx,
            )?),
            Box::new(syntax_expr_to_constructor_pattern(
                expr.children.get(1)?,
                bindings,
                ctx,
            )?),
        )),
        _ => None,
    }
}

/// Returns the source name represented by an expression when it is name-like.
///
/// Inputs:
/// - `expr`: syntax-output expression.
///
/// Output:
/// - Source text for atom/name or variable expressions.
/// - `None` for non-name expressions.
///
/// Transformation:
/// - Normalizes the few expression kinds that can act as call heads or module
///   names in the transitional syntax bridge.
pub(super) fn syntax_expr_name(expr: &SyntaxExprOutput) -> Option<&str> {
    match expr.kind {
        SyntaxExprKind::Atom | SyntaxExprKind::Var => expr.text.as_deref(),
        _ => None,
    }
}
