use super::*;

/// Validates that a SQL form row type names a visible type.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be a compiler-known `sql` raw
///   macro.
/// - `ctx`: expression inference context containing local structs, local
///   aliases, and imported type names.
/// - `errors`: mutable expression error sink.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Reads the SQL-form analysis payload and emits a source-spanned diagnostic
///   when `sql[RowType]` does not reference a visible local struct, local type
///   alias, or imported type. SQL result columns are intentionally left for the
///   later Postgres-backed validation path.
pub(super) fn validate_sql_form_row_type(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    let Ok(Some(analysis)) = crate::sql_forms::analyze_sql_form(expr) else {
        return;
    };
    if analysis.row_type_arg_count != 1 {
        return;
    }
    let Some(row_type) = analysis.row_type.as_deref() else {
        return;
    };

    let Some((module, name)) = sql_row_type_reference(row_type) else {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!("SQL row type `{row_type}` must be a named visible row type"),
        ));
        return;
    };

    if sql_row_type_is_visible(module.as_deref(), &name, ctx) {
        validate_sql_form_row_projection(expr, row_type, module.as_deref(), &name, ctx, errors);
        return;
    }

    errors.push(spanned_expression_error(
        expr.span.into(),
        format!("SQL row type `{row_type}` is not a visible struct, type alias, or imported type"),
    ));
}

/// Validates simple SQL projections against local row struct fields.
///
/// Inputs:
/// - `expr`: SQL raw macro expression.
/// - `row_type`: source row type text used in diagnostics.
/// - `module`: optional module path from the row type.
/// - `name`: base row type name.
/// - `ctx`: expression inference context containing local struct fields.
/// - `errors`: mutable expression error sink.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - For visible local struct row types and simple `SELECT` / `RETURNING`
///   projections, compares selected field names with declared struct fields.
///   Complex SQL, imported row types, and alias-backed rows are left for the
///   Postgres-backed validation path.
fn validate_sql_form_row_projection(
    expr: &SyntaxExprOutput,
    row_type: &str,
    module: Option<&str>,
    name: &str,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    if module.is_some() {
        return;
    }
    let Some(row_fields) = ctx.struct_fields.get(name) else {
        return;
    };
    let Some(raw) = expr.raw.as_deref() else {
        return;
    };
    let Some(selected_fields) = crate::sql_forms::simple_sql_projection_fields(raw) else {
        return;
    };

    let selected = selected_fields.iter().cloned().collect::<HashSet<_>>();
    let mut unknown = selected_fields
        .iter()
        .filter(|field| !row_fields.contains_key(*field))
        .cloned()
        .collect::<Vec<_>>();
    unknown.sort();
    unknown.dedup();
    for field in unknown {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!("SQL selected column `{field}` is not a field on row type `{row_type}`"),
        ));
    }

    let mut missing = row_fields
        .keys()
        .filter(|field| !selected.contains(*field))
        .cloned()
        .collect::<Vec<_>>();
    missing.sort();
    for field in missing {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!("SQL row type `{row_type}` field `{field}` is not selected by this query"),
        ));
    }
}

/// Infers the wrapper result type for a ready SQL form.
///
/// Inputs:
/// - `expr`: syntax-output expression that may be a compiler-known `sql` raw
///   macro.
/// - `ctx`: expression inference context containing visible type metadata.
/// - `errors`: mutable expression error sink for malformed internal wrapper
///   type text.
///
/// Output:
/// - Parsed wrapper result type for ready SQL forms with visible row types.
/// - `None` for non-SQL forms, blocked SQL forms, or SQL forms whose row type
///   is not visible.
///
/// Transformation:
/// - Reuses the backend-neutral SQL wrapper plan and checks row-type
///   visibility, then constructs the structural `Result`/`Option` shape
///   directly. This keeps SQL forms independent of whether the source module
///   imported the public `Result` or `Option` aliases.
pub(super) fn infer_sql_form_result_type(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    _errors: &mut Vec<String>,
) -> Option<Type> {
    let plan = crate::sql_forms::build_sql_wrapper_plan(expr, expr.children.len())
        .ok()
        .flatten()?;
    let (module, name) = sql_row_type_reference(&plan.row_type)?;
    if !sql_row_type_is_visible(module.as_deref(), &name, ctx) {
        return None;
    }

    let row_type = parse_sql_row_type(&plan.row_type, ctx, &name).unwrap_or_else(|| Type::Named {
        module,
        name,
        args: Vec::new(),
    });
    Some(sql_result_type_for_cardinality(plan.cardinality, row_type))
}

/// Parses a SQL row type annotation into a typechecker type.
///
/// Inputs:
/// - `row_type`: row type text captured from SQL metadata.
/// - `ctx`: expression inference context with visible aliases.
/// - `row_type_name`: fallback alias name for SQL row visibility.
///
/// Output:
/// - Parsed row type when it can be resolved.
///
/// Transformation:
/// - Builds the SQL alias scope and delegates type-expression parsing.
fn parse_sql_row_type(row_type: &str, ctx: &ExprInferContext, row_type_name: &str) -> Option<Type> {
    let mut vars = HashMap::new();
    let mut next_var = 0;
    let alias_names = sql_result_type_alias_names(ctx, row_type_name);
    parse_type_expr(row_type, &alias_names, &mut vars, &mut next_var)
}

/// Wraps a SQL row type according to query cardinality.
///
/// Inputs:
/// - `cardinality`: SQL query cardinality inferred from the macro.
/// - `row_type`: decoded row type.
///
/// Output:
/// - Terlan type representing the query result payload.
///
/// Transformation:
/// - Converts optional rows to `Option`, multi-row queries to lists, and
///   execute-style queries to affected-row counts.
fn sql_result_type_for_cardinality(
    cardinality: crate::sql_forms::SqlCardinality,
    row_type: Type,
) -> Type {
    let ok_value = match cardinality {
        crate::sql_forms::SqlCardinality::OptionalOne => normalize_union(vec![
            Type::LiteralAtom("none".to_string()),
            some_type(row_type),
        ]),
        crate::sql_forms::SqlCardinality::ManyRows => Type::List(Box::new(row_type)),
        crate::sql_forms::SqlCardinality::AffectedRows => Type::Int,
        crate::sql_forms::SqlCardinality::Ambiguous => Type::Dynamic,
    };
    normalize_union(vec![
        Type::Tuple(vec![Type::LiteralAtom("ok".to_string()), ok_value]),
        Type::Tuple(vec![Type::LiteralAtom("error".to_string()), Type::Dynamic]),
    ])
}

fn some_type(value: Type) -> Type {
    Type::Tuple(vec![Type::LiteralAtom("some".to_string()), value])
}

/// Builds the visible type-name set used for SQL wrapper result parsing.
///
/// Inputs:
/// - `ctx`: expression inference context.
/// - `row_type_name`: unqualified row type name from the SQL wrapper plan.
///
/// Output:
/// - Type-name set passed to the normal type parser.
///
/// Transformation:
/// - Combines local aliases, local structs, imported type names, primitives,
///   standard wrapper types, and the row type itself so wrapper result text like
///   `Result[Option[UserRow], Error]` parses to named types instead of fresh
///   inference variables.
fn sql_result_type_alias_names(ctx: &ExprInferContext, row_type_name: &str) -> HashSet<String> {
    let mut alias_names = ctx.aliases.keys().cloned().collect::<HashSet<_>>();
    alias_names.extend(ctx.struct_fields.keys().cloned());
    alias_names.extend(ctx.imported_type_names.keys().cloned());
    alias_names.extend(primitive_type_names());
    alias_names.extend(
        ["Result", "Option", "Error", row_type_name]
            .into_iter()
            .map(str::to_string),
    );
    alias_names
}

/// Extracts the named type reference from a SQL row type argument.
///
/// Inputs:
/// - `row_type`: source-like type text preserved by syntax output.
///
/// Output:
/// - Module path plus base name for simple or generic named row type
///   references.
/// - `None` when the row type text is not shaped like a named type.
///
/// Transformation:
/// - Removes a top-level generic argument list, splits the remaining name at
///   the final dot, and rejects structural type spellings that cannot name a
///   row type declaration.
fn sql_row_type_reference(row_type: &str) -> Option<(Option<String>, String)> {
    let row_type = row_type.trim();
    if row_type.is_empty()
        || row_type.starts_with('[')
        || row_type.starts_with('{')
        || row_type.starts_with('(')
        || row_type.starts_with(':')
        || row_type.contains('|')
    {
        return None;
    }

    let name = row_type.split_once('[').map_or(row_type, |(name, _)| name);
    if name.is_empty()
        || !name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return None;
    }

    let (module, base) = split_module_name(name);
    if base.is_empty() {
        None
    } else {
        Some((module, base))
    }
}

/// Returns whether a SQL row type reference is visible in this module.
///
/// Inputs:
/// - `module`: optional module path from the row type text.
/// - `name`: base row type name.
/// - `ctx`: expression inference context containing visible type metadata.
///
/// Output:
/// - `true` when the name resolves to a local struct, local/imported alias, or
///   imported type.
///
/// Transformation:
/// - Uses existing typechecker visibility tables instead of inferring row type
///   validity from capitalization alone.
fn sql_row_type_is_visible(module: Option<&str>, name: &str, ctx: &ExprInferContext) -> bool {
    if module.is_none()
        && (ctx.struct_fields.contains_key(name)
            || ctx.aliases.contains_key(name)
            || ctx.imported_type_names.contains_key(name))
    {
        return true;
    }

    module.is_some_and(|module| {
        ctx.imported_type_names
            .values()
            .any(|imported| imported.module == module && imported.name == name)
    })
}
