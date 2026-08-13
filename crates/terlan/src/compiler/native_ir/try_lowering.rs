//! Bounded native lowering for status-aware `Try` expressions.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreCaseClause, CoreExpr, CoreFunction};

use super::structured_case::{
    bind_values, bool_and, extend_bindings, pattern_plan, validate_bindings,
};
use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

const MAX_TRY_CLAUSES: usize = 256;
const FAILURE_STATUS_TYPE: NativeType = NativeType::Int;

/// Clause selection values shared by success and failure arms of one `try`.
struct TryClauseSelection<'a> {
    clauses: &'a [CoreCaseClause],
    value: NativeExpr,
    value_type: NativeType,
    result_type: NativeType,
    value_slot: usize,
    default_to_value: bool,
}

/// Lexical and application-wide lookup tables used by `try` clause lowering.
#[derive(Clone, Copy)]
struct TryLoweringEnvironment<'a> {
    params: &'a HashMap<String, usize>,
    param_types: &'a HashMap<String, NativeType>,
    functions: &'a HashMap<(String, usize), usize>,
    function_types: &'a HashMap<(String, usize), NativeType>,
    constructors: &'a NativeConstructorLayouts,
}

/// Lowers a top-level non-suspending protected region into local status control flow.
pub(super) fn lower_try(
    expr: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::Try {
        body,
        of_clauses,
        catch_clauses,
        after_clause,
    } = expr
    else {
        return Ok(None);
    };
    if of_clauses.len() > MAX_TRY_CLAUSES || catch_clauses.len() > MAX_TRY_CLAUSES {
        return Err(format!(
            "error[native_ir.try_clauses]: try clauses exceed {MAX_TRY_CLAUSES}"
        ));
    }
    if catch_clauses.is_empty() {
        return Err("error[native_ir.try_catch]: native Try requires a catch clause".into());
    }

    let protected_type =
        infer_native_type_with_constructors(body, param_types, function_types, constructors)
            .ok_or_else(|| {
                "error[native_ir.try_body_type]: protected type is unavailable".to_string()
            })?;
    let protected = lower_expr_with_constructors(
        body,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    let result_type = super::native_return_type(function).ok_or_else(|| {
        "error[native_ir.try_result_type]: result type is unavailable".to_string()
    })?;
    let value_slot = params.values().copied().max().map_or(0, |slot| slot + 1);
    let environment = TryLoweringEnvironment {
        params,
        param_types,
        functions,
        function_types,
        constructors,
    };
    let success = lower_clauses(
        TryClauseSelection {
            clauses: of_clauses,
            value: NativeExpr::Param(value_slot),
            value_type: protected_type,
            result_type,
            value_slot,
            default_to_value: true,
        },
        environment,
    )?;
    let failure = lower_clauses(
        TryClauseSelection {
            clauses: catch_clauses,
            value: NativeExpr::Param(value_slot),
            value_type: FAILURE_STATUS_TYPE,
            result_type,
            value_slot,
            default_to_value: false,
        },
        environment,
    )?;
    let cleanup = after_clause
        .iter()
        .flat_map(|after| [&*after.trigger, &*after.body])
        .map(|expression| {
            lower_expr_with_constructors(
                expression,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(NativeExpr::Try {
        protected: Box::new(protected),
        success: Box::new(success),
        failure: Box::new(failure),
        cleanup,
    }))
}
fn lower_clauses(
    selection: TryClauseSelection<'_>,
    environment: TryLoweringEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let TryClauseSelection {
        clauses,
        value,
        value_type,
        result_type,
        value_slot,
        default_to_value,
    } = selection;
    let TryLoweringEnvironment {
        params,
        param_types,
        functions,
        function_types,
        constructors,
    } = environment;
    if clauses.is_empty() && default_to_value && value_type == result_type {
        return Ok(value);
    }
    if clauses.is_empty() {
        return Err("error[native_ir.try_clauses]: native Try has no selectable clauses".into());
    }
    let mut lowered = Vec::with_capacity(clauses.len());
    for clause in clauses {
        let plan = pattern_plan(
            &clause.pattern,
            value.clone(),
            value_type,
            None,
            constructors,
            0,
        )?;
        validate_bindings(&plan.bindings)?;
        let (slots, types) = extend_bindings(
            params,
            param_types,
            value_slot.saturating_add(1),
            &plan.bindings,
        );
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                lower_expr_with_constructors(
                    guard,
                    &slots,
                    &types,
                    functions,
                    function_types,
                    constructors,
                )
            })
            .transpose()?
            .unwrap_or(NativeExpr::Bool(true));
        let body = lower_expr_with_constructors(
            &clause.body,
            &slots,
            &types,
            functions,
            function_types,
            constructors,
        )?;
        let body_type =
            infer_native_type_with_constructors(&clause.body, &types, function_types, constructors)
                .ok_or_else(|| {
                    "error[native_ir.try_clause_type]: clause type is unavailable".to_string()
                })?;
        if body_type != result_type {
            return Err(
                "error[native_ir.try_clause_type]: clause result disagrees with function result"
                    .into(),
            );
        }
        lowered.push((
            bool_and(plan.predicate, bind_values(&plan.bindings, guard)),
            bind_values(&plan.bindings, body),
        ));
    }
    Ok(NativeExpr::If { clauses: lowered })
}
