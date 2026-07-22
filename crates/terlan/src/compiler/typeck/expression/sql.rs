use super::*;

mod abi;

use crate::database_schema::DatabaseColumnCodec;
use crate::terlan_typeck::sql_forms::database::SqlDatabaseProjectionColumn;
use abi::{
    sql_row_decode_type_is_supported, sql_scalar_abi_type_is_supported, structural_option_inner,
    SQL_ROW_DECODE_ABI_TYPE_SUMMARY, SQL_SCALAR_ABI_TYPE_SUMMARY,
};

/// Validates inferred SQL interpolation types against the first VM binding ABI.
///
/// Inputs:
/// - `expr`: compiler-known SQL form containing interpolation children.
/// - `parameter_types`: normal type-inference result for each child in source order.
/// - `ctx`: expression context containing visible aliases.
/// - `subst`: substitutions learned while inferring all interpolation children.
/// - `errors`: mutable expression diagnostic sink.
///
/// Output:
/// - No direct return value; one indexed, source-spanned error is emitted for
///   every parameter that cannot cross the current scalar SQL boundary.
///
/// Transformation:
/// - Applies final substitutions and transparent aliases before accepting the
///   runtime's JSON-safe scalar set. Structured values, nullable wrappers,
///   dynamic values, functions, and unresolved generics remain rejected until
///   their database codecs are explicit compiler/runtime contracts.
pub(super) fn validate_sql_form_parameter_types(
    expr: &SyntaxExprOutput,
    parameter_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    if expr.kind != SyntaxExprKind::RawMacro || expr.text.as_deref() != Some("sql") {
        return;
    }
    debug_assert_eq!(expr.children.len(), parameter_types.len());

    for (index, (parameter, inferred)) in expr.children.iter().zip(parameter_types).enumerate() {
        let resolved = expand_type_aliases(&apply_subst(inferred, subst), ctx.aliases);
        if sql_scalar_abi_type_is_supported(&resolved) {
            continue;
        }
        errors.push(spanned_expression_error(
            parameter.span.into(),
            format!(
                "SQL parameter {} has non-bindable type {}; expected {}",
                index + 1,
                pretty_type(&resolved),
                SQL_SCALAR_ABI_TYPE_SUMMARY
            ),
        ));
    }
}

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
/// - Reads the SQL-form analysis payload and emits source-spanned diagnostics
///   unless `sql[RowType]` resolves to a visible named row or a non-empty tuple
///   whose fields cross the current scalar decode ABI. Local struct fields and
///   AST-visible projection arity are checked before database-backed validation.
pub(super) fn validate_sql_form_row_type(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    let Ok(Some(analysis)) = crate::terlan_typeck::sql_forms::analyze_sql_form(expr) else {
        return;
    };
    if analysis.row_type_arg_count != 1 {
        return;
    }
    let Some(row_type) = analysis.row_type.as_deref() else {
        return;
    };

    let database_projection = match ctx.database_schema {
        Some(snapshot) => {
            let Ok(statement) = crate::terlan_typeck::sql_forms::parse_single_postgres_statement(
                &analysis.binding.sql,
            ) else {
                return;
            };
            match crate::terlan_typeck::sql_forms::statement_schema_projection(
                &statement, &snapshot,
            ) {
                Ok(projection) => projection,
                Err(error) => {
                    errors.push(spanned_expression_error(expr.span.into(), error.message()));
                    return;
                }
            }
        }
        None => None,
    };
    let database_field_names = database_projection.as_ref().map(|projection| {
        projection
            .iter()
            .map(|column| column.output_name.clone())
            .collect::<Vec<_>>()
    });
    let selected_fields = database_field_names
        .as_deref()
        .or(analysis.projection_fields.as_deref());

    let Some((parsed, resolved)) = parse_sql_row_descriptor(row_type, ctx) else {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!("SQL row type `{row_type}` is not a valid type expression"),
        ));
        return;
    };

    match resolved {
        Type::Tuple(items) => validate_sql_tuple_row_descriptor(
            expr,
            row_type,
            &items,
            selected_fields,
            database_projection.as_deref(),
            ctx,
            errors,
        ),
        Type::Named { module, name, .. }
            if sql_row_type_is_visible(module.as_deref(), &name, ctx) =>
        {
            validate_sql_form_row_projection(
                expr,
                row_type,
                module.as_deref(),
                &name,
                selected_fields,
                ctx,
                errors,
            );
            validate_sql_struct_row_field_types(
                expr,
                row_type,
                module.as_deref(),
                &name,
                database_projection.as_deref(),
                ctx,
                errors,
            );
        }
        Type::Named { .. } => errors.push(spanned_expression_error(
            expr.span.into(),
            format!(
                "SQL row type `{row_type}` is not a visible struct, type alias, or imported type"
            ),
        )),
        _ => errors.push(spanned_expression_error(
            expr.span.into(),
            format!(
                "SQL row type `{}` must resolve to a visible named row type or non-empty scalar tuple",
                pretty_type(&parsed)
            ),
        )),
    }
}

fn validate_sql_tuple_row_descriptor(
    expr: &SyntaxExprOutput,
    row_type: &str,
    items: &[Type],
    selected_fields: Option<&[String]>,
    database_projection: Option<&[SqlDatabaseProjectionColumn]>,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    if items.is_empty() {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!("SQL tuple row type `{row_type}` must contain at least one field"),
        ));
        return;
    }

    if let Some(selected_fields) = selected_fields {
        if selected_fields.len() != items.len() {
            errors.push(spanned_expression_error(
                expr.span.into(),
                format!(
                    "SQL projection has {} column(s), but tuple row type `{row_type}` has {} field(s)",
                    selected_fields.len(),
                    items.len()
                ),
            ));
        }
    }

    for (index, item) in items.iter().enumerate() {
        if !sql_row_decode_type_is_supported_in_context(item, ctx) {
            errors.push(spanned_expression_error(
                expr.span.into(),
                format!(
                    "SQL tuple row field {} has non-decodable type {}; expected {}",
                    index + 1,
                    pretty_type(item),
                    SQL_ROW_DECODE_ABI_TYPE_SUMMARY
                ),
            ));
        }
    }

    if let Some(projection) = database_projection {
        if projection.len() == items.len() {
            for (index, (column, item)) in projection.iter().zip(items).enumerate() {
                validate_sql_database_column_type(
                    expr,
                    column,
                    item,
                    &format!("tuple row field {}", index + 1),
                    ctx,
                    errors,
                );
            }
        }
    }
}

fn validate_sql_struct_row_field_types(
    expr: &SyntaxExprOutput,
    row_type: &str,
    module: Option<&str>,
    name: &str,
    database_projection: Option<&[SqlDatabaseProjectionColumn]>,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    if module.is_some() {
        return;
    }
    let Some(row_fields) = ctx.struct_fields.get(name) else {
        return;
    };
    let mut fields = row_fields.iter().collect::<Vec<_>>();
    fields.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (field, ty) in fields {
        let resolved = expand_type_aliases(ty, ctx.aliases);
        if !sql_row_decode_type_is_supported_in_context(&resolved, ctx) {
            errors.push(spanned_expression_error(
                expr.span.into(),
                format!(
                    "SQL row type `{row_type}` field `{field}` has non-decodable type {}; expected {}",
                    pretty_type(&resolved),
                    SQL_ROW_DECODE_ABI_TYPE_SUMMARY
                ),
            ));
        }
    }

    let Some(projection) = database_projection else {
        return;
    };
    for column in projection {
        let Some(field_type) = row_fields.get(&column.output_name) else {
            continue;
        };
        let resolved = expand_type_aliases(field_type, ctx.aliases);
        validate_sql_database_column_type(
            expr,
            column,
            &resolved,
            &format!("row type `{row_type}` field `{}`", column.output_name),
            ctx,
            errors,
        );
    }
}

fn validate_sql_database_column_type(
    expr: &SyntaxExprOutput,
    column: &SqlDatabaseProjectionColumn,
    row_field_type: &Type,
    row_field_label: &str,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    if !sql_row_decode_type_is_supported_in_context(row_field_type, ctx) {
        return;
    }
    let Some(codec) = DatabaseColumnCodec::for_schema_column(&column.source_column) else {
        errors.push(spanned_expression_error(
            expr.span.into(),
            format!(
                "SQL selected column `{}` uses unsupported PostgreSQL type `{}`; no typed Terlan row codec is available",
                column.output_name,
                column.source_column.qualified_database_type()
            ),
        ));
        return;
    };

    let (actual_nullable, actual_base) = structural_option_inner(row_field_type)
        .map_or((false, row_field_type), |inner| (true, inner));
    let expected_nullable = column.source_column.nullable;
    if actual_nullable == expected_nullable
        && sql_database_codec_matches_type(codec, actual_base, ctx)
    {
        return;
    }

    let base = codec.terlan_type_name();
    let expected = if expected_nullable {
        format!("Option[{base}]")
    } else {
        base.to_string()
    };
    errors.push(spanned_expression_error(
        expr.span.into(),
        format!(
            "SQL selected column `{}` decodes as {expected}, but {row_field_label} has type {}",
            column.output_name,
            pretty_type(row_field_type)
        ),
    ));
}

fn sql_database_codec_matches_type(
    codec: DatabaseColumnCodec,
    ty: &Type,
    ctx: &ExprInferContext,
) -> bool {
    match codec {
        DatabaseColumnCodec::Binary => matches!(ty, Type::Binary),
        DatabaseColumnCodec::Bool => matches!(ty, Type::Bool),
        DatabaseColumnCodec::Int => matches!(ty, Type::Int),
        DatabaseColumnCodec::Json => sql_json_type_is_visible(ty, ctx),
    }
}

fn sql_row_decode_type_is_supported_in_context(ty: &Type, ctx: &ExprInferContext) -> bool {
    sql_row_decode_type_is_supported(ty)
        || sql_json_type_is_visible(ty, ctx)
        || structural_option_inner(ty).is_some_and(|inner| sql_json_type_is_visible(inner, ctx))
}

fn sql_json_type_is_visible(ty: &Type, ctx: &ExprInferContext) -> bool {
    let Type::Named { module, name, args } = ty else {
        return false;
    };
    if name != "Json" || !args.is_empty() {
        return false;
    }
    match module.as_deref() {
        Some("std.data.Json") => true,
        Some(_) => false,
        None => ctx
            .imported_type_names
            .get("Json")
            .is_some_and(|imported| imported.module == "std.data.Json" && imported.name == "Json"),
    }
}

/// Validates simple SQL projections against local row struct fields.
///
/// Inputs:
/// - `expr`: SQL raw macro expression.
/// - `row_type`: source row type text used in diagnostics.
/// - `module`: optional module path from the row type.
/// - `name`: base row type name.
/// - `selected_fields`: compiler-visible output names derived from the SQL AST.
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
    selected_fields: Option<&[String]>,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) {
    if module.is_some() {
        return;
    }
    let Some(row_fields) = ctx.struct_fields.get(name) else {
        return;
    };
    let Some(selected_fields) = selected_fields else {
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
    let plan = crate::terlan_typeck::sql_forms::build_sql_wrapper_plan(expr, expr.children.len())
        .ok()
        .flatten()?;
    let (parsed, resolved) = parse_sql_row_descriptor(&plan.row_type, ctx)?;
    if !sql_row_descriptor_is_supported(&resolved, ctx) {
        return None;
    }
    Some(sql_result_type_for_cardinality(plan.cardinality, parsed))
}

fn parse_sql_row_descriptor(row_type: &str, ctx: &ExprInferContext) -> Option<(Type, Type)> {
    let row_type_name = sql_row_type_reference(row_type)
        .map(|(_, name)| name)
        .unwrap_or_default();
    let parsed = parse_sql_row_type(row_type, ctx, &row_type_name)?;
    let resolved = expand_type_aliases(&parsed, ctx.aliases);
    Some((parsed, resolved))
}

fn sql_row_descriptor_is_supported(row_type: &Type, ctx: &ExprInferContext) -> bool {
    match row_type {
        Type::Tuple(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| sql_row_decode_type_is_supported_in_context(item, ctx))
        }
        Type::Named { module, name, .. } => sql_row_type_is_visible(module.as_deref(), name, ctx),
        _ => false,
    }
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
    cardinality: crate::terlan_typeck::sql_forms::SqlCardinality,
    row_type: Type,
) -> Type {
    let ok_value = match cardinality {
        crate::terlan_typeck::sql_forms::SqlCardinality::OptionalOne => normalize_union(vec![
            Type::LiteralAtom("none".to_string()),
            some_type(row_type),
        ]),
        crate::terlan_typeck::sql_forms::SqlCardinality::ManyRows => Type::List(Box::new(row_type)),
        crate::terlan_typeck::sql_forms::SqlCardinality::AffectedRows => Type::Int,
        crate::terlan_typeck::sql_forms::SqlCardinality::Ambiguous => Type::Dynamic,
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
