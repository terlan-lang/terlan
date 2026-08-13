//! Lexical routing for structured cases into their bounded NativeIR matcher.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CorePattern, CoreType};

use super::{
    bind_values, bool_and, core_expr_type, extend_bindings, pattern_plan, validate_bindings,
};
use crate::compiler::native_ir::{
    infer_native_type_with_constructors, lower_expr_with_constructors, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

const MAX_STRUCTURED_CASE_CLAUSES: usize = 256;

/// Application-wide callable and constructor knowledge used by case lowering.
#[derive(Clone, Copy)]
pub(crate) struct StructuredCaseEnvironment<'a> {
    pub(crate) functions: &'a HashMap<(String, usize), usize>,
    pub(crate) function_types: &'a HashMap<(String, usize), NativeType>,
    pub(crate) function_core_types: &'a HashMap<(String, usize), CoreType>,
    pub(crate) constructors: &'a NativeConstructorLayouts,
}

pub(crate) fn lower_structured_case(
    body: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<Option<NativeExpr>, crate::compiler::native_ir::NativeIrError> {
    if !contains_case(body) {
        return Ok(None);
    }
    let core_types = function
        .params
        .iter()
        .filter_map(|parameter| {
            parameter
                .core_ty
                .as_ref()
                .map(|ty| (parameter.name.clone(), ty.clone()))
        })
        .collect::<HashMap<_, _>>();
    Ok(Some(lower_containing_case(
        body,
        params,
        param_types,
        &core_types,
        environment,
    )?))
}
fn lower_containing_case(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let StructuredCaseEnvironment {
        function_types,
        function_core_types,
        constructors,
        ..
    } = environment;
    match expr {
        CoreExpr::Case { scrutinee, clauses } => lower_case(
            scrutinee,
            clauses,
            params,
            param_types,
            core_types,
            environment,
        ),
        CoreExpr::Let { bindings, body } => {
            let mut locals = params.clone();
            let mut local_types = param_types.clone();
            let mut local_core_types = core_types.clone();
            let mut next_local = locals
                .values()
                .copied()
                .max()
                .map_or(0, |slot| slot.saturating_add(1));
            let mut lowered = Vec::with_capacity(bindings.len());
            for (binding_index, binding) in bindings.iter().enumerate() {
                let CorePattern::Var(name) = &binding.pattern else {
                    return Err(
                        "error[native_ir.structured_case_let_pattern]: structured case lexical prefix requires variable bindings".into(),
                    );
                };
                if binding_index + 1 == bindings.len() {
                    if let CoreExpr::Case {
                        scrutinee,
                        clauses,
                    } = body.as_ref()
                    {
                        if matches!(scrutinee.as_ref(), CoreExpr::Var(scrutinee) if scrutinee == name)
                        {
                            if let Some((items, item_core_types)) = tuple_scrutinee(
                                &binding.value,
                                &local_core_types,
                                function_core_types,
                            ) {
                                if clauses.iter().all(|clause| {
                                    matches!(&clause.pattern, CorePattern::Tuple(patterns) if patterns.len() == items.len())
                                        || matches!(clause.pattern, CorePattern::Wildcard)
                                }) {
                                    let case = lower_scalar_tuple_case(
                                        items,
                                        item_core_types,
                                        clauses,
                                        &locals,
                                        &local_types,
                                        &local_core_types,
                                        environment,
                                    )?;
                                    return Ok(if lowered.is_empty() {
                                        case
                                    } else {
                                        NativeExpr::Let {
                                            bindings: lowered,
                                            body: Box::new(case),
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
                let structured = contains_case(&binding.value);
                let ty = if structured {
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
                        core_expr_type(
                            &binding.value,
                            &local_core_types,
                            function_core_types,
                        )
                        .and_then(|ty| {
                            crate::compiler::native_ir::native_type(
                                Some(&ty),
                                &ty.contract_text(),
                            )
                        })
                    })
                    .or_else(|| {
                        closure_invocation_signature(&binding.value, &local_core_types)
                            .map(|(_, result)| result)
                    })
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.structured_case_let_type]: cannot infer binding `{name}`"
                        )
                    })?
                };
                lowered.push(if structured {
                    lower_child(
                        &binding.value,
                        &locals,
                        &local_types,
                        &local_core_types,
                        environment,
                    )?
                } else {
                    lower_plain(
                        &binding.value,
                        &locals,
                        &local_types,
                        &local_core_types,
                        environment,
                    )?
                });
                locals.insert(name.clone(), next_local);
                local_types.insert(name.clone(), ty);
                if let Some(core_type) =
                    core_expr_type(&binding.value, &local_core_types, function_core_types)
                {
                    local_core_types.insert(name.clone(), core_type);
                }
                next_local = next_local.saturating_add(1);
            }
            let body = lower_child(
                body,
                &locals,
                &local_types,
                &local_core_types,
                environment,
            )?;
            Ok(if lowered.is_empty() {
                body
            } else {
                NativeExpr::Let {
                    bindings: lowered,
                    body: Box::new(body),
                }
            })
        }
        CoreExpr::If { clauses } if !clauses.is_empty() => Ok(NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    Ok((
                        lower_child(
                            &clause.condition,
                            params,
                            param_types,
                            core_types,
                            environment,
                        )?,
                        lower_child(
                            &clause.body,
                            params,
                            param_types,
                            core_types,
                            environment,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        _ => Err(
            "error[native_ir.structured_case_route]: structured case is nested in an unsupported expression"
                .into(),
        ),
    }
}
fn lower_case(
    scrutinee: &CoreExpr,
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let StructuredCaseEnvironment {
        function_types,
        function_core_types,
        constructors,
        ..
    } = environment;
    if clauses.is_empty() || clauses.len() > MAX_STRUCTURED_CASE_CLAUSES {
        return Err(format!(
            "error[native_ir.structured_case_clauses]: structured case has {} clauses; limit is {MAX_STRUCTURED_CASE_CLAUSES}",
            clauses.len()
        ));
    }
    if let Some((items, item_core_types)) =
        tuple_scrutinee(scrutinee, core_types, function_core_types)
    {
        if clauses.iter().all(|clause| {
            matches!(&clause.pattern, CorePattern::Tuple(patterns) if patterns.len() == items.len())
                || matches!(clause.pattern, CorePattern::Wildcard)
        }) {
            return lower_scalar_tuple_case(
                items,
                item_core_types,
                clauses,
                params,
                param_types,
                core_types,
                environment,
            );
        }
    }
    let scrutinee_type =
        infer_native_type_with_constructors(scrutinee, param_types, function_types, constructors)
            .or_else(|| {
                core_expr_type(scrutinee, core_types, function_core_types).and_then(|ty| {
                    crate::compiler::native_ir::native_type(Some(&ty), &ty.contract_text())
                })
            })
            .ok_or_else(|| {
                format!(
                    "error[native_ir.structured_case_type]: unknown scrutinee type for `{}`",
                    scrutinee.contract_text()
                )
            })?;
    let scrutinee_core = core_expr_type(scrutinee, core_types, function_core_types);
    let scrutinee = lower_plain(scrutinee, params, param_types, core_types, environment)?;
    let scrutinee_slot = params
        .values()
        .copied()
        .max()
        .map_or(0, |slot| slot.saturating_add(1));
    let mut outer_slots = params.clone();
    let mut outer_types = param_types.clone();
    outer_slots.insert(
        "$native_structured_case_scrutinee".to_string(),
        scrutinee_slot,
    );
    outer_types.insert(
        "$native_structured_case_scrutinee".to_string(),
        scrutinee_type,
    );
    let scrutinee_value = NativeExpr::Param(scrutinee_slot);

    let mut native_clauses = Vec::with_capacity(clauses.len());
    for clause in clauses {
        let plan = pattern_plan(
            &clause.pattern,
            scrutinee_value.clone(),
            scrutinee_type,
            scrutinee_core.as_ref(),
            constructors,
            0,
        )?;
        validate_bindings(&plan.bindings)?;
        let (binding_slots, binding_types) = extend_bindings(
            &outer_slots,
            &outer_types,
            scrutinee_slot.saturating_add(1),
            &plan.bindings,
        );
        let binding_core_types =
            plan.bindings
                .iter()
                .fold(core_types.clone(), |mut types, binding| {
                    if let Some(core_ty) = &binding.core_ty {
                        types.insert(binding.name.clone(), core_ty.clone());
                    }
                    types
                });
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                lower_child(
                    guard,
                    &binding_slots,
                    &binding_types,
                    &binding_core_types,
                    environment,
                )
            })
            .transpose()?
            .unwrap_or(NativeExpr::Bool(true));
        let condition = bool_and(plan.predicate, bind_values(&plan.bindings, guard));
        let selected = lower_child(
            &clause.body,
            &binding_slots,
            &binding_types,
            &binding_core_types,
            environment,
        )?;
        native_clauses.push((condition, bind_values(&plan.bindings, selected)));
    }
    Ok(NativeExpr::Let {
        bindings: vec![scrutinee],
        body: Box::new(NativeExpr::If {
            clauses: native_clauses,
        }),
    })
}

fn tuple_scrutinee<'a>(
    expr: &'a CoreExpr,
    core_types: &HashMap<String, CoreType>,
    function_core_types: &HashMap<(String, usize), CoreType>,
) -> Option<(&'a [CoreExpr], Vec<CoreType>)> {
    match expr {
        CoreExpr::Cast {
            expr,
            target_type: CoreType::Tuple(elements),
        } => {
            let CoreExpr::Tuple(items) = expr.as_ref() else {
                return None;
            };
            (items.len() == elements.len()).then(|| {
                (
                    items.as_slice(),
                    elements
                        .iter()
                        .map(|element| match element {
                            crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
                            | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                                ty.clone()
                            }
                        })
                        .collect(),
                )
            })
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| core_expr_type(item, core_types, function_core_types))
            .collect::<Option<Vec<_>>>()
            .map(|types| (items.as_slice(), types)),
        _ => None,
    }
}
fn lower_scalar_tuple_case(
    items: &[CoreExpr],
    item_core_types: Vec<CoreType>,
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let StructuredCaseEnvironment {
        function_types,
        constructors,
        ..
    } = environment;
    let item_types = items
        .iter()
        .zip(&item_core_types)
        .map(|(item, core_type)| {
            infer_native_type_with_constructors(item, param_types, function_types, constructors)
                .or_else(|| {
                    crate::compiler::native_ir::native_type(
                        Some(core_type),
                        &core_type.contract_text(),
                    )
                })
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.structured_tuple_item_type]: unknown tuple item type for `{}`",
                        item.contract_text()
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let lowered_items = items
        .iter()
        .map(|item| lower_plain(item, params, param_types, core_types, environment))
        .collect::<Result<Vec<_>, _>>()?;
    let first_item_slot = params
        .values()
        .copied()
        .max()
        .map_or(0, |slot| slot.saturating_add(1));
    let mut outer_slots = params.clone();
    let mut outer_types = param_types.clone();
    for (index, item_type) in item_types.iter().copied().enumerate() {
        let name = format!("$native_structured_tuple_item_{index}");
        outer_slots.insert(name.clone(), first_item_slot + index);
        outer_types.insert(name, item_type);
    }

    let mut native_clauses = Vec::with_capacity(clauses.len());
    for clause in clauses {
        let mut predicate = NativeExpr::Bool(true);
        let mut bindings = Vec::new();
        if let CorePattern::Tuple(patterns) = &clause.pattern {
            for (index, ((pattern, item_type), item_core_type)) in patterns
                .iter()
                .zip(item_types.iter().copied())
                .zip(item_core_types.iter())
                .enumerate()
            {
                let plan = pattern_plan(
                    pattern,
                    NativeExpr::Param(first_item_slot + index),
                    item_type,
                    Some(item_core_type),
                    constructors,
                    0,
                )?;
                predicate = bool_and(predicate, plan.predicate);
                bindings.extend(plan.bindings);
            }
        }
        validate_bindings(&bindings)?;
        let first_binding_slot = first_item_slot + items.len();
        let (binding_slots, binding_types) =
            extend_bindings(&outer_slots, &outer_types, first_binding_slot, &bindings);
        let binding_core_types = bindings
            .iter()
            .fold(core_types.clone(), |mut types, binding| {
                if let Some(core_ty) = &binding.core_ty {
                    types.insert(binding.name.clone(), core_ty.clone());
                }
                types
            });
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                lower_child(
                    guard,
                    &binding_slots,
                    &binding_types,
                    &binding_core_types,
                    environment,
                )
            })
            .transpose()?
            .unwrap_or(NativeExpr::Bool(true));
        let condition = bool_and(predicate, bind_values(&bindings, guard));
        let selected = lower_child(
            &clause.body,
            &binding_slots,
            &binding_types,
            &binding_core_types,
            environment,
        )?;
        native_clauses.push((condition, bind_values(&bindings, selected)));
    }
    Ok(NativeExpr::Let {
        bindings: lowered_items,
        body: Box::new(NativeExpr::If {
            clauses: native_clauses,
        }),
    })
}
fn lower_child(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    if matches!(expr, CoreExpr::FunctionCall { .. }) {
        return lower_plain(expr, params, param_types, core_types, environment);
    }
    if contains_case(expr) || contains_indirect_call(expr) {
        lower_containing_case(expr, params, param_types, core_types, environment)
    } else {
        lower_plain(expr, params, param_types, core_types, environment)
    }
}

/// Lowers one checked expression with its complete lexical type environment.
///
/// Suspension-aware control prefixes use this entry point for values that can
/// include indirect closure calls or nested structured cases before the
/// continuation boundary is assembled.
pub(in crate::compiler::native_ir) fn lower_lexical_expr(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    lower_child(expr, params, param_types, core_types, environment)
}

fn contains_indirect_call(expr: &CoreExpr) -> bool {
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

pub(super) fn lower_plain(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<NativeExpr, String> {
    let StructuredCaseEnvironment {
        functions,
        function_types,
        function_core_types,
        constructors,
    } = environment;
    if matches!(expr, CoreExpr::Tuple(_)) {
        if let Some(core_type) = core_expr_type(expr, core_types, function_core_types) {
            if let Some(lowered) =
                crate::compiler::native_ir::collection_values::lower_boundary_collection_value(
                    expr,
                    Some(&core_type),
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
            {
                return Ok(lowered);
            }
        }
    }
    if let CoreExpr::FunctionCall { callee, args } = expr {
        let (parameter_types, result_type) = closure_invocation_signature(expr, core_types)
            .ok_or_else(|| {
                "error[native_ir.structured_case_closure]: indirect call has no checked arrow type"
                    .to_string()
            })?;
        if parameter_types.len() != args.len() {
            return Err(
                "error[native_ir.structured_case_closure]: indirect call arity mismatch".into(),
            );
        }
        return Ok(NativeExpr::InvokeClosure {
            callee: Box::new(lower_expr_with_constructors(
                callee,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?),
            args: args
                .iter()
                .map(|arg| {
                    lower_expr_with_constructors(
                        arg,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            parameter_types,
            result_type,
        });
    }
    lower_expr_with_constructors(
        expr,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
}

fn closure_invocation_signature(
    expr: &CoreExpr,
    core_types: &HashMap<String, CoreType>,
) -> Option<(Vec<NativeType>, NativeType)> {
    let CoreExpr::FunctionCall { callee, .. } = expr else {
        return None;
    };
    let CoreExpr::Var(name) = callee.as_ref() else {
        return None;
    };
    let CoreType::Arrow {
        params,
        return_type,
    } = core_types.get(name)?
    else {
        return None;
    };
    let params = params
        .iter()
        .map(|ty| super::super::native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()?;
    let result = super::super::native_type(Some(return_type), &return_type.contract_text())?;
    Some((params, result))
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
