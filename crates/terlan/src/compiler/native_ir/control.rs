//! Suspension-aware control lowering for Terlan NativeIR.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreIfClause};

use super::{
    composed_call_region, condition_yield_region, constructors::NativeConstructorLayouts,
    expr_is_scalar, free_variables, lower_expr_with_constructors, lower_yield_region,
    rebase_callee_locals, rewrite_linear_suspension, stable_continuation_id, yield_region,
    CallRegion, ComposedCallProfile, NativeContinuation, NativeExpr, NativeType, YieldRegion,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_expr_with_yields(
    expr: &CoreExpr,
    param_names: &[String],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending_functions: &HashSet<(String, usize)>,
    terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    module: &str,
    function: &str,
    arity: usize,
    return_type: NativeType,
    ordinal: &mut usize,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    if let CoreExpr::Call { function, args } = expr {
        let identity = (function.clone(), args.len());
        if suspending_functions.contains(&identity) {
            let function = functions.get(&identity).copied().ok_or_else(|| {
                format!(
                    "error[native_ir.tail_call]: suspending call `{}/{}` is not in the native module",
                    identity.0, identity.1
                )
            })?;
            let args = args
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
                .collect::<Result<Vec<_>, _>>()?;
            return Ok((NativeExpr::TailCall { function, args }, Vec::new()));
        }
    }

    let is_terminal = |name: &str, call_arity: usize| {
        functions
            .get(&(name.to_string(), call_arity))
            .is_some_and(|index| terminal_profiles.contains_key(index))
    };
    let reserved_names = params.keys().cloned().collect::<HashSet<_>>();
    if let Some(region) =
        composed_call_region(expr, suspending_functions, &is_terminal, &reserved_names)
    {
        return lower_prepared_call(
            &region,
            param_names,
            params,
            param_types,
            functions,
            function_types,
            constructors,
            suspending_functions,
            terminal_profiles,
            module,
            function,
            arity,
            return_type,
            ordinal,
            stable_ids,
        );
    }

    if let Some(region) = condition_yield_region(expr) {
        return lower_prepared_yield(
            &region,
            param_names,
            params,
            param_types,
            functions,
            function_types,
            constructors,
            suspending_functions,
            terminal_profiles,
            module,
            function,
            arity,
            return_type,
            ordinal,
            stable_ids,
        );
    }

    if let CoreExpr::Let { bindings, body } = expr {
        if super::contains_process_yield(body) {
            let mut entry_names = param_names.to_vec();
            let mut entry_vars = params.clone();
            let mut entry_types = param_types.clone();
            let mut entry_bindings = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
                    return Err("error[native_ir.control_prefix]: \
                         native control prefix requires variable bindings"
                        .to_string());
                };
                let value_type = super::infer_native_type_with_constructors(
                    &binding.value,
                    &entry_types,
                    function_types,
                    constructors,
                )
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.control_prefix]: cannot infer native type for `{name}`"
                    )
                })?;
                let value = lower_expr_with_constructors(
                    &binding.value,
                    &entry_vars,
                    &entry_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                let index = params.len() + entry_bindings.len();
                entry_names.retain(|entry| entry != name);
                entry_names.push(name.clone());
                entry_vars.insert(name.clone(), index);
                entry_types.insert(name.clone(), value_type);
                entry_bindings.push(value);
            }
            let (body, continuations) = lower_expr_with_yields(
                body,
                &entry_names,
                &entry_vars,
                &entry_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                ordinal,
                stable_ids,
            )?;
            return Ok((
                NativeExpr::Let {
                    bindings: entry_bindings,
                    body: Box::new(body),
                },
                continuations,
            ));
        }
    }

    if let CoreExpr::If { clauses } = expr {
        let Some((first, remaining)) = clauses.split_first() else {
            return Err("error[native_ir.if]: native conditional has no clauses".to_string());
        };
        if let Some(condition_region) = condition_yield_region(&first.condition) {
            let mut resumed_clauses = Vec::with_capacity(clauses.len());
            resumed_clauses.push(CoreIfClause {
                condition: condition_region.resume,
                body: first.body.clone(),
            });
            resumed_clauses.extend_from_slice(remaining);
            let region = YieldRegion {
                prefix: condition_region.prefix,
                operation: condition_region.operation,
                arguments: condition_region.arguments,
                result: condition_region.result,
                resume: CoreExpr::If {
                    clauses: resumed_clauses,
                },
            };
            return lower_prepared_yield(
                &region,
                param_names,
                params,
                param_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                ordinal,
                stable_ids,
            );
        }

        if let Some(mut call_region) = composed_call_region(
            &first.condition,
            suspending_functions,
            &is_terminal,
            &reserved_names,
        ) {
            let mut resumed_clauses = Vec::with_capacity(clauses.len());
            resumed_clauses.push(CoreIfClause {
                condition: call_region.resume,
                body: first.body.clone(),
            });
            resumed_clauses.extend_from_slice(remaining);
            call_region.resume = CoreExpr::If {
                clauses: resumed_clauses,
            };
            return lower_prepared_call(
                &call_region,
                param_names,
                params,
                param_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                ordinal,
                stable_ids,
            );
        }

        if let CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } = &first.condition
        {
            if matches!(operator.as_str(), "and" | "&&" | "or" | "||")
                && !expr_is_scalar(&first.condition)
            {
                let left = lower_expr_with_constructors(
                    left,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                let mut right_clauses = Vec::with_capacity(clauses.len());
                right_clauses.push(CoreIfClause {
                    condition: right.as_ref().clone(),
                    body: first.body.clone(),
                });
                right_clauses.extend_from_slice(remaining);
                let (right_path, mut continuations) = lower_expr_with_yields(
                    &CoreExpr::If {
                        clauses: right_clauses,
                    },
                    param_names,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                    ordinal,
                    stable_ids,
                )?;
                if matches!(operator.as_str(), "and" | "&&") {
                    let (fallback, fallback_continuations) = if remaining.is_empty() {
                        (
                            NativeExpr::If {
                                clauses: Vec::new(),
                            },
                            Vec::new(),
                        )
                    } else {
                        lower_expr_with_yields(
                            &CoreExpr::If {
                                clauses: remaining.to_vec(),
                            },
                            param_names,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                            suspending_functions,
                            terminal_profiles,
                            module,
                            function,
                            arity,
                            return_type,
                            ordinal,
                            stable_ids,
                        )?
                    };
                    continuations.extend(fallback_continuations);
                    return Ok((
                        NativeExpr::If {
                            clauses: vec![(left, right_path), (NativeExpr::Bool(true), fallback)],
                        },
                        continuations,
                    ));
                }
                let (selected, selected_continuations) = lower_expr_with_yields(
                    &first.body,
                    param_names,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                    ordinal,
                    stable_ids,
                )?;
                continuations.extend(selected_continuations);
                return Ok((
                    NativeExpr::If {
                        clauses: vec![(left, selected), (NativeExpr::Bool(true), right_path)],
                    },
                    continuations,
                ));
            }
        }

        let condition = lower_expr_with_constructors(
            &first.condition,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?;
        let (body, mut continuations) = lower_expr_with_yields(
            &first.body,
            param_names,
            params,
            param_types,
            functions,
            function_types,
            constructors,
            suspending_functions,
            terminal_profiles,
            module,
            function,
            arity,
            return_type,
            ordinal,
            stable_ids,
        )?;
        let mut lowered_clauses = vec![(condition, body)];
        if !remaining.is_empty() {
            let (fallback, fallback_continuations) = lower_expr_with_yields(
                &CoreExpr::If {
                    clauses: remaining.to_vec(),
                },
                param_names,
                params,
                param_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                ordinal,
                stable_ids,
            )?;
            lowered_clauses.push((NativeExpr::Bool(true), fallback));
            continuations.extend(fallback_continuations);
        }
        return Ok((
            NativeExpr::If {
                clauses: lowered_clauses,
            },
            continuations,
        ));
    }

    if let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = expr
    {
        if matches!(operator.as_str(), "and" | "&&" | "or" | "||") {
            if let Some(left_region) = yield_region(left) {
                let region = YieldRegion {
                    prefix: left_region.prefix,
                    operation: left_region.operation,
                    arguments: left_region.arguments,
                    result: left_region.result,
                    resume: CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: Box::new(left_region.resume),
                        right: right.clone(),
                    },
                };
                return lower_prepared_yield(
                    &region,
                    param_names,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                    ordinal,
                    stable_ids,
                );
            }
            let left = lower_expr_with_constructors(
                left,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            let (right, continuations) = lower_expr_with_yields(
                right,
                param_names,
                params,
                param_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                ordinal,
                stable_ids,
            )?;
            let clauses = if matches!(operator.as_str(), "and" | "&&") {
                vec![
                    (left, right),
                    (NativeExpr::Bool(true), NativeExpr::Bool(false)),
                ]
            } else {
                vec![
                    (left, NativeExpr::Bool(true)),
                    (NativeExpr::Bool(true), right),
                ]
            };
            return Ok((NativeExpr::If { clauses }, continuations));
        }
    }

    Ok((
        lower_expr_with_constructors(
            expr,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?,
        Vec::new(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn lower_prepared_call(
    region: &CallRegion,
    param_names: &[String],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending_functions: &HashSet<(String, usize)>,
    terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    module: &str,
    function: &str,
    arity: usize,
    return_type: NativeType,
    ordinal: &mut usize,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    let identity = (region.function.clone(), region.args.len());
    let function_index = functions.get(&identity).copied().ok_or_else(|| {
        format!(
            "error[native_ir.call_then]: composed call `{}/{}` is not in the native module",
            identity.0, identity.1
        )
    })?;
    let profile = terminal_profiles.get(&function_index).ok_or_else(|| {
        format!(
            "error[native_ir.call_then]: composed call `{}/{}` has no continuation profile",
            identity.0, identity.1
        )
    })?;
    let mut wrapper_ids = Vec::with_capacity(profile.continuations.len());
    for _ in &profile.continuations {
        let continuation_id = stable_continuation_id(module, function, arity, *ordinal);
        *ordinal = ordinal.saturating_add(1);
        if !stable_ids.insert(continuation_id) {
            return Err(format!(
                "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{module}`"
            ));
        }
        wrapper_ids.push(continuation_id);
    }

    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_bindings = Vec::with_capacity(region.prefix.len());
    let mut prefix_names = Vec::with_capacity(region.prefix.len());
    for binding in &region.prefix {
        let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.call_prefix]: composed call prefix requires scalar variable bindings"
                    .to_string(),
            );
        };
        if entry_vars.contains_key(name) {
            return Err(format!(
                "error[native_ir.call_prefix]: composed call prefix shadows scalar `{name}`"
            ));
        }
        let value_type = super::infer_native_type_with_constructors(
            &binding.value,
            &entry_types,
            function_types,
            constructors,
        )
        .ok_or_else(|| {
            format!("error[native_ir.call_prefix]: cannot infer scalar prefix `{name}`")
        })?;
        let value = lower_expr_with_constructors(
            &binding.value,
            &entry_vars,
            &entry_types,
            functions,
            function_types,
            constructors,
        )?;
        entry_bindings.push(value);
        entry_vars.insert(name.clone(), params.len() + entry_bindings.len() - 1);
        entry_types.insert(name.clone(), value_type);
        prefix_names.push(name.clone());
    }

    let mut captures = free_variables(&region.resume);
    captures.remove(&region.result_name);
    let capture_names = param_names
        .iter()
        .chain(prefix_names.iter())
        .filter(|name| captures.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(unknown) = captures.iter().find(|name| !entry_vars.contains_key(*name)) {
        return Err(format!(
            "error[native_ir.call_capture]: continuation references unavailable scalar `{unknown}`"
        ));
    }
    let values = capture_names
        .iter()
        .map(|name| {
            entry_vars
                .get(name)
                .copied()
                .map(NativeExpr::Param)
                .ok_or_else(|| {
                    format!("error[native_ir.call_capture]: scalar `{name}` was not materialized")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut immediate_vars = entry_vars.clone();
    immediate_vars.insert(region.result_name.clone(), entry_vars.len());
    let capture_types = capture_names
        .iter()
        .map(|name| {
            entry_types.get(name).copied().ok_or_else(|| {
                format!("error[native_ir.call_type]: scalar `{name}` has no native type")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result_type = function_types.get(&identity).copied().ok_or_else(|| {
        format!(
            "error[native_ir.call_type]: composed call `{}/{}` has no native return type",
            identity.0, identity.1
        )
    })?;
    let mut immediate_types = entry_types.clone();
    immediate_types.insert(region.result_name.clone(), result_type);
    let mut resume_names = capture_names.clone();
    resume_names.push(region.result_name.clone());
    let nested_ordinal_start = *ordinal;
    let nested_ids_start = stable_ids.clone();
    let (immediate_resume, nested_continuations) = lower_expr_with_yields(
        &region.resume,
        &resume_names,
        &immediate_vars,
        &immediate_types,
        functions,
        function_types,
        constructors,
        suspending_functions,
        terminal_profiles,
        module,
        function,
        arity,
        return_type,
        ordinal,
        stable_ids,
    )?;
    let mut wrappers = Vec::with_capacity(profile.continuations.len());
    for (index, callee) in profile.continuations.iter().enumerate() {
        let callee_capture_count = callee.params.len();
        let mut continuation_params = callee.params.clone();
        continuation_params.extend(capture_types.iter().copied());
        let callee_body =
            rebase_callee_locals(&callee.body, callee_capture_count, capture_names.len());
        let body = if let Some(next_callee) = profile.continuations.get(index + 1) {
            rewrite_linear_suspension(
                &callee_body,
                next_callee.id,
                wrapper_ids[index + 1],
                callee_capture_count,
                capture_names.len(),
            )?
        } else {
            let mut resume_vars = capture_names
                .iter()
                .enumerate()
                .map(|(capture_index, name)| (name.clone(), callee_capture_count + capture_index))
                .collect::<HashMap<_, _>>();
            resume_vars.insert(region.result_name.clone(), continuation_params.len());
            let mut resume_types = capture_names
                .iter()
                .zip(capture_types.iter().copied())
                .map(|(name, ty)| (name.clone(), ty))
                .collect::<HashMap<_, _>>();
            resume_types.insert(region.result_name.clone(), result_type);
            let mut wrapper_ordinal = nested_ordinal_start;
            let mut wrapper_stable_ids = nested_ids_start.clone();
            let (resumed_caller, wrapper_nested) = lower_expr_with_yields(
                &region.resume,
                &resume_names,
                &resume_vars,
                &resume_types,
                functions,
                function_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                module,
                function,
                arity,
                return_type,
                &mut wrapper_ordinal,
                &mut wrapper_stable_ids,
            )?;
            if wrapper_ordinal != *ordinal || wrapper_nested != nested_continuations {
                return Err(
                    "error[native_ir.call_chain]: immediate and resumed nested continuation layouts differ"
                        .to_string(),
                );
            }
            NativeExpr::Let {
                bindings: vec![callee_body],
                body: Box::new(resumed_caller),
            }
        };
        wrappers.push(NativeContinuation {
            id: wrapper_ids[index],
            params: continuation_params,
            return_type,
            body,
        });
    }
    wrappers.extend(nested_continuations);
    let first_callee = &profile.continuations[0];
    let args = region
        .args
        .iter()
        .map(|arg| {
            lower_expr_with_constructors(
                arg,
                &entry_vars,
                &entry_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let call = NativeExpr::CallThen {
        function: function_index,
        args,
        callee_continuation_id: first_callee.id,
        callee_capture_count: first_callee.params.len(),
        continuation_id: wrapper_ids[0],
        values,
        resume: Box::new(immediate_resume),
    };
    Ok((
        if entry_bindings.is_empty() {
            call
        } else {
            NativeExpr::Let {
                bindings: entry_bindings,
                body: Box::new(call),
            }
        },
        wrappers,
    ))
}

#[allow(clippy::too_many_arguments)]
fn lower_prepared_yield(
    region: &YieldRegion,
    param_names: &[String],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending_functions: &HashSet<(String, usize)>,
    terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    module: &str,
    function: &str,
    arity: usize,
    return_type: NativeType,
    ordinal: &mut usize,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    let continuation_id = stable_continuation_id(module, function, arity, *ordinal);
    *ordinal = ordinal.saturating_add(1);
    if !stable_ids.insert(continuation_id) {
        return Err(format!(
            "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{module}`"
        ));
    }
    let lowered = lower_yield_region(
        region,
        param_names,
        params,
        param_types,
        functions,
        function_types,
        constructors,
        continuation_id,
    )?;
    let (resume_body, nested) = lower_expr_with_yields(
        &lowered.resume,
        &lowered.resume_names,
        &lowered.resume_vars,
        &lowered.resume_types,
        functions,
        function_types,
        constructors,
        suspending_functions,
        terminal_profiles,
        module,
        function,
        arity,
        return_type,
        ordinal,
        stable_ids,
    )?;
    let mut continuations = Vec::with_capacity(nested.len() + 1);
    continuations.push(NativeContinuation {
        id: continuation_id,
        params: lowered.continuation_params,
        return_type,
        body: resume_body,
    });
    continuations.extend(nested);
    Ok((lowered.entry, continuations))
}
