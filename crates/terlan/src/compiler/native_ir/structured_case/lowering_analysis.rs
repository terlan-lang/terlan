//! Shape and result analysis for structured NativeIR cases.

use std::collections::HashMap;

use crate::compiler::native_ir::{infer_native_type_with_constructors, NativeExpr, NativeType};
use crate::terlan_typeck::{CoreExpr, CorePattern, CoreType};

use super::{core_expr_type, pattern_plan, type_excludes_pattern, StructuredCaseEnvironment};

pub(super) fn contains_indirect_call(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::FunctionCall { .. } => true,
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| contains_indirect_call(&binding.value))
                || contains_indirect_call(body)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            contains_indirect_call(&clause.condition) || contains_indirect_call(&clause.body)
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            contains_indirect_call(scrutinee)
                || clauses.iter().any(|clause| {
                    clause.guard.as_ref().is_some_and(contains_indirect_call)
                        || contains_indirect_call(&clause.body)
                })
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            contains_indirect_call(left) || contains_indirect_call(right)
        }
        _ => false,
    }
}

/// Infers the native result type of a structured expression in its lexical environment.
pub(in crate::compiler::native_ir) fn structured_result_type(
    expr: &CoreExpr,
    types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeType, String> {
    let StructuredCaseEnvironment {
        function_types,
        function_core_types,
        constructors,
        ..
    } = environment;
    match expr {
        CoreExpr::Let { bindings, body } => {
            let mut local_types = types.clone();
            let mut local_core_types = core_types.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return Err(
                        "error[native_ir.structured_case_result_pattern]: result binding must be a variable"
                            .into(),
                    );
                };
                let ty = if contains_case(&binding.value) {
                    structured_result_type(
                        &binding.value,
                        &local_types,
                        &local_core_types,
                        environment,
                    )?
                } else {
                    infer_native_type_with_constructors(
                        &binding.value,
                        &local_types,
                        function_types,
                        constructors,
                    )
                    .or_else(|| {
                        core_expr_type(&binding.value, &local_core_types, function_core_types)
                            .and_then(|ty| {
                                crate::compiler::native_ir::native_type(
                                    Some(&ty),
                                    &ty.contract_text(),
                                )
                            })
                    })
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.structured_case_result_type]: cannot infer binding `{name}`"
                        )
                    })?
                };
                local_types.insert(name.clone(), ty);
                if let Some(core_type) =
                    core_expr_type(&binding.value, &local_core_types, function_core_types)
                {
                    local_core_types.insert(name.clone(), core_type);
                }
            }
            structured_result_type(body, &local_types, &local_core_types, environment)
        }
        CoreExpr::Case { scrutinee, clauses } if !clauses.is_empty() => {
            let scrutinee_type =
                infer_native_type_with_constructors(scrutinee, types, function_types, constructors)
                    .or_else(|| {
                        core_expr_type(scrutinee, core_types, function_core_types).and_then(|ty| {
                            crate::compiler::native_ir::native_type(
                                Some(&ty),
                                &ty.contract_text(),
                            )
                        })
                    })
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.structured_case_result_scrutinee]: unknown type for {scrutinee:?}"
                        )
                    })?;
            let scrutinee_core = core_expr_type(scrutinee, core_types, function_core_types);
            let mut result = None;
            for clause in clauses {
                if type_excludes_pattern(&clause.pattern, scrutinee_core.as_ref()) {
                    continue;
                }
                let plan = pattern_plan(
                    &clause.pattern,
                    NativeExpr::Param(0),
                    scrutinee_type,
                    scrutinee_core.as_ref(),
                    constructors,
                    0,
                )?;
                let clause_types =
                    plan.bindings
                        .iter()
                        .fold(types.clone(), |mut types, binding| {
                            types.insert(binding.name.clone(), binding.ty);
                            types
                        });
                let clause_core_types =
                    plan.bindings
                        .iter()
                        .fold(core_types.clone(), |mut types, binding| {
                            if let Some(core_ty) = &binding.core_ty {
                                types.insert(binding.name.clone(), core_ty.clone());
                            }
                            types
                        });
                let ty = structured_result_type(
                    &clause.body,
                    &clause_types,
                    &clause_core_types,
                    environment,
                )?;
                if result.is_some_and(|expected| expected != ty) {
                    return Err(
                        "error[native_ir.structured_case_result_type]: case clauses have different native types"
                            .into(),
                    );
                }
                result = Some(ty);
            }
            result.ok_or_else(|| {
                "error[native_ir.structured_case_result_type]: case has no result".into()
            })
        }
        CoreExpr::If { clauses } if !clauses.is_empty() => {
            let mut result = None;
            for clause in clauses {
                let ty = structured_result_type(&clause.body, types, core_types, environment)?;
                if result.is_some_and(|expected| expected != ty) {
                    return Err(
                        "error[native_ir.structured_case_result_type]: if clauses have different native types"
                            .into(),
                    );
                }
                result = Some(ty);
            }
            result.ok_or_else(|| {
                "error[native_ir.structured_case_result_type]: if has no result".into()
            })
        }
        _ => infer_native_type_with_constructors(expr, types, function_types, constructors)
            .ok_or_else(|| {
                format!(
                    "error[native_ir.structured_case_result_type]: cannot infer result for {expr:?}"
                )
            }),
    }
}

pub(crate) fn contains_case(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Case { .. } => true,
        CoreExpr::Let { bindings, body } => {
            bindings.iter().any(|binding| contains_case(&binding.value)) || contains_case(body)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .any(|clause| contains_case(&clause.condition) || contains_case(&clause.body)),
        CoreExpr::BinaryOp { left, right, .. } => contains_case(left) || contains_case(right),
        _ => false,
    }
}
