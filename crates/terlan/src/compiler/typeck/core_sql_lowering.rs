use super::core_expr_lowering::core_expr_from_syntax;
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
///   expressions, query kind, transaction requirement, cardinality, result
///   type, and simple projection fields at the backend-neutral CoreIR boundary
///   without emitting backend code.
pub fn sql_query_core_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let plan = crate::terlan_typeck::sql_forms::build_sql_wrapper_plan(expr, expr.children.len())
        .ok()
        .flatten()?;
    let parameters = expr
        .children
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;
    debug_assert_eq!(parameters.len(), plan.parameter_count);

    Some(CoreExpr::SqlQuery {
        row_type: plan.row_type,
        bound_sql: plan.bound_sql,
        parameters,
        query_kind: plan.query_kind.as_diagnostic_label().to_string(),
        transaction_requirement: plan
            .transaction_requirement
            .as_diagnostic_label()
            .to_string(),
        cardinality: plan.cardinality.as_diagnostic_label().to_string(),
        result_type: plan.result_type,
        projection_fields: plan.projection_fields.unwrap_or_default(),
    })
}
