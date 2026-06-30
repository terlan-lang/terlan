use super::*;

/// Converts a ready syntax-output SQL raw macro into a CoreIR query payload.
///
/// Inputs:
/// - `expr`: syntax-output raw macro expression, expected to be `sql[Row]`.
///
/// Output:
/// - `Some(CoreExpr::SqlQuery)` when the SQL form has a wrapper plan.
/// - `None` for non-SQL raw macros or SQL forms blocked by wrapper readiness.
///
/// Transformation:
/// - Reuses SQL wrapper analysis to preserve row type, bound SQL, parameter
///   count, cardinality, result type, and simple projection fields at the
///   backend-neutral CoreIR boundary without emitting backend code.
pub fn sql_query_core_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let plan = crate::terlan_typeck::sql_forms::build_sql_wrapper_plan(expr, expr.children.len())
        .ok()
        .flatten()?;

    Some(CoreExpr::SqlQuery {
        row_type: plan.row_type,
        bound_sql: plan.bound_sql,
        parameter_count: plan.parameter_count,
        cardinality: plan.cardinality.as_diagnostic_label().to_string(),
        result_type: plan.result_type,
        projection_fields: plan.projection_fields.unwrap_or_default(),
    })
}
