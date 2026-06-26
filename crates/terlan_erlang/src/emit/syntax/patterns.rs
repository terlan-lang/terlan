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

/// Collects value names bound by a syntax-output pattern.
///
/// Inputs:
/// - `pattern`: source pattern that controls a local scope.
/// - `locals`: mutable local-name set to extend.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Walks nested tuple/list/map/record/constructor pattern structure and adds
///   every ordinary variable binding. Boolean literals and wildcard-like
///   patterns do not introduce value locals.
pub(super) fn collect_syntax_pattern_value_locals(
    pattern: &SyntaxPatternOutput,
    locals: &mut BTreeSet<String>,
) {
    if matches!(pattern.kind, SyntaxPatternKind::Var) {
        if let Some(name) = pattern.text.as_deref() {
            if !is_bool_literal_name(name) {
                locals.insert(name.to_string());
            }
        }
    }

    for child in &pattern.children {
        collect_syntax_pattern_value_locals(child, locals);
    }
    for field in &pattern.fields {
        collect_syntax_pattern_value_locals(&field.value, locals);
    }
}

/// Collects inferred value types for names bound by a pattern.
///
/// Inputs:
/// - `pattern`: source pattern that controls a local scope.
/// - `type_text`: inferred source type matched by the pattern.
/// - `value_types`: mutable local type table to extend.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Propagates simple structural type information through visible alias
///   constructor, tuple, and list patterns. This lets method calls on
///   destructured values keep using primitive receiver lowering even before
///   full typed syntax annotations are available on pattern nodes.
pub(super) fn collect_syntax_pattern_value_types(
    pattern: &SyntaxPatternOutput,
    type_text: &str,
    ctx: &SyntaxLowerCtx,
    value_types: &mut BTreeMap<String, String>,
) {
    if matches!(pattern.kind, SyntaxPatternKind::Var) {
        if let Some(name) = pattern.text.as_deref() {
            if !is_bool_literal_name(name) {
                value_types.insert(name.to_string(), normalize_trait_type_text(type_text));
                return;
            }
        }
    }

    match pattern.kind {
        SyntaxPatternKind::Constructor => {
            if let Some(name) = pattern.text.as_deref() {
                if ctx
                    .alias_constructor_target(name, pattern.children.len())
                    .is_some()
                {
                    let type_args = named_type_args(type_text);
                    for (child, child_type) in pattern.children.iter().zip(type_args.iter()) {
                        collect_syntax_pattern_value_types(child, child_type, ctx, value_types);
                    }
                }
            }
        }
        SyntaxPatternKind::Tuple => {
            if let Some(types) = tuple_type_args(type_text) {
                for (child, child_type) in pattern.children.iter().zip(types.iter()) {
                    collect_syntax_pattern_value_types(child, child_type, ctx, value_types);
                }
            }
        }
        SyntaxPatternKind::List => {
            if let Some(element_type) = first_named_type_arg(type_text) {
                for child in &pattern.children {
                    collect_syntax_pattern_value_types(child, &element_type, ctx, value_types);
                }
            }
        }
        SyntaxPatternKind::ListCons => {
            if let Some(element_type) = first_named_type_arg(type_text) {
                for child in &pattern.children {
                    collect_syntax_pattern_value_types(child, &element_type, ctx, value_types);
                }
            }
        }
        SyntaxPatternKind::Map | SyntaxPatternKind::Record | SyntaxPatternKind::MapField => {}
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder
        | SyntaxPatternKind::Var
        | SyntaxPatternKind::Int
        | SyntaxPatternKind::Float
        | SyntaxPatternKind::Atom => {}
    }
}

/// Extracts the first named type argument from type text.
///
/// Inputs:
/// - `type_text`: normalized or source-shaped type text.
///
/// Output:
/// - First type argument when the type is an application.
///
/// Transformation:
/// - Reuses named type argument parsing and drops all but the first argument.
fn first_named_type_arg(type_text: &str) -> Option<String> {
    named_type_args(type_text).into_iter().next()
}

/// Extracts normalized arguments from a named type application.
///
/// Inputs:
/// - `type_text`: type text such as `Option[Int]`.
///
/// Output:
/// - Normalized type argument text in declaration order.
///
/// Transformation:
/// - Compacts whitespace/application syntax, parses the argument list, and
///   normalizes each nested type.
fn named_type_args(type_text: &str) -> Vec<String> {
    let compact = compact_type_application(&compact_spaces(type_text));
    parse_named_type_args(&compact)
        .map(|(_, args)| {
            args.into_iter()
                .map(|arg| normalize_trait_type_text(&arg))
                .collect()
        })
        .unwrap_or_default()
}

/// Extracts normalized element types from tuple type text.
///
/// Inputs:
/// - `type_text`: type text expected to be shaped like `{A, B}`.
///
/// Output:
/// - Tuple element type text when the input is a tuple.
///
/// Transformation:
/// - Strips tuple delimiters and splits top-level comma-separated elements.
fn tuple_type_args(type_text: &str) -> Option<Vec<String>> {
    let compact = compact_type_application(&compact_spaces(type_text));
    let inner = compact.strip_prefix('{')?.strip_suffix('}')?;
    Some(
        split_top_level_type_args(inner)
            .into_iter()
            .map(|arg| normalize_trait_type_text(&arg))
            .collect(),
    )
}

/// Splits a type-argument list on top-level commas.
///
/// Inputs:
/// - `src`: inner type-argument text without surrounding delimiters.
///
/// Output:
/// - Trimmed type argument segments.
///
/// Transformation:
/// - Tracks nested delimiter depth so commas inside tuples, calls, or generic
///   applications do not split the outer list.
fn split_top_level_type_args(src: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in src.char_indices() {
        match ch {
            '[' | '{' | '<' | '(' => depth += 1,
            ']' | '}' | '>' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let arg = src[start..index].trim();
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let arg = src[start..].trim();
    if !arg.is_empty() {
        args.push(arg.to_string());
    }
    args
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
