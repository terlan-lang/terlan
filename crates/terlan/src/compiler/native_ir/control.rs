//! Suspension-aware control lowering for Terlan NativeIR.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_list_empty_operation, encode_list_prepend_operation,
};
use crate::terlan_typeck::{CoreExpr, CoreIfClause, CoreType};

use super::{
    call_composition::{CallTarget, DynamicCallProfiles, DynamicCallSignature},
    composed_call_region, condition_yield_region,
    constructors::NativeConstructorLayouts,
    contains_process_yield,
    control_completion::{complete, CompletionTarget},
    expr_calls_suspending, expr_is_scalar, free_variables, lower_expr_with_constructors,
    lower_yield_region, stable_composed_completion_id, stable_continuation_id, yield_region,
    CallRegion, ComposedCallProfile, NativeContinuation, NativeExpr, NativeType, YieldRegion,
    YieldRegionEnvironment, YieldRegionRequest,
};

/// Returns the first unoccupied NativeIR local even when hidden continuation
/// parameters make the source-variable map sparse.
pub(super) fn next_local_index(params: &HashMap<String, usize>) -> usize {
    params
        .values()
        .copied()
        .max()
        .map_or(0, |index| index.saturating_add(1))
}

#[derive(Clone, Copy)]
pub(super) struct YieldLoweringScope<'a> {
    pub(super) param_names: &'a [String],
    pub(super) params: &'a HashMap<String, usize>,
    pub(super) param_types: &'a HashMap<String, NativeType>,
    pub(super) param_core_types: &'a HashMap<String, CoreType>,
    pub(super) completion: Option<&'a CompletionTarget>,
}

#[derive(Clone, Copy)]
pub(super) struct YieldLoweringEnvironment<'a> {
    pub(super) functions: &'a HashMap<(String, usize), usize>,
    pub(super) function_types: &'a HashMap<(String, usize), NativeType>,
    pub(super) function_core_types: &'a HashMap<(String, usize), CoreType>,
    pub(super) constructors: &'a NativeConstructorLayouts,
    pub(super) suspending_functions: &'a HashSet<(String, usize)>,
    pub(super) terminal_profiles: &'a HashMap<usize, ComposedCallProfile>,
    pub(super) dynamic_profiles: &'a DynamicCallProfiles,
    pub(super) module: &'a str,
    pub(super) function: &'a str,
    pub(super) arity: usize,
    pub(super) return_type: NativeType,
}

pub(super) struct YieldLoweringState<'a> {
    pub(super) ordinal: &'a mut usize,
    pub(super) stable_ids: &'a mut HashSet<u64>,
}

/// Recovers the native result type of one lexical expression, including
/// structured cases whose pattern bindings are not visible to scalar inference.
pub(super) fn lexical_result_type(
    expr: &CoreExpr,
    native_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: &YieldLoweringEnvironment<'_>,
) -> Option<NativeType> {
    super::infer_native_type_with_constructors(
        expr,
        native_types,
        environment.function_types,
        environment.constructors,
    )
    .or_else(|| {
        super::structured_case::core_expr_type(expr, core_types, environment.function_core_types)
            .and_then(|core_type| super::native_type(Some(&core_type), &core_type.contract_text()))
    })
    .or_else(|| {
        super::structured_case::contains_case(expr)
            .then(|| {
                super::structured_case::structured_result_type(
                    expr,
                    native_types,
                    core_types,
                    super::structured_case::StructuredCaseEnvironment {
                        functions: environment.functions,
                        function_types: environment.function_types,
                        function_core_types: environment.function_core_types,
                        constructors: environment.constructors,
                    },
                )
                .ok()
            })
            .flatten()
    })
}

pub(super) fn lower_expr_with_yields(
    expr: &CoreExpr,
    scope: YieldLoweringScope<'_>,
    environment: &YieldLoweringEnvironment<'_>,
    state: &mut YieldLoweringState<'_>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    lower_owned_expr_with_yields(expr.clone(), scope, environment, state)
}
pub(super) fn lower_owned_expr_with_yields(
    owned_expr: CoreExpr,
    scope: YieldLoweringScope<'_>,
    environment: &YieldLoweringEnvironment<'_>,
    state: &mut YieldLoweringState<'_>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    let YieldLoweringScope {
        param_names,
        params,
        param_types,
        param_core_types,
        completion,
    } = scope;
    let YieldLoweringEnvironment {
        functions,
        function_types,
        function_core_types,
        constructors,
        suspending_functions,
        terminal_profiles,
        dynamic_profiles,
        module,
        function,
        arity,
        return_type,
    } = *environment;
    let ordinal = &mut *state.ordinal;
    let stable_ids = &mut *state.stable_ids;
    let expr = &owned_expr;
    let lower_current_lexical = |value: &CoreExpr| {
        super::structured_case::lower_lexical_expr(
            value,
            params,
            param_types,
            param_core_types,
            super::structured_case::StructuredCaseEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
            },
        )
    };
    if let CoreExpr::Call { function, args } = expr {
        let identity = (function.clone(), args.len());
        if suspending_functions.contains(&identity)
            && completion.is_none()
            && args.iter().all(|argument| {
                !expr_calls_suspending(argument, suspending_functions)
                    && !contains_process_yield(argument)
            })
        {
            let function = functions.get(&identity).copied().ok_or_else(|| {
                format!(
                    "error[native_ir.tail_call]: suspending call `{}/{}` is not in the native module",
                    identity.0, identity.1
                )
            })?;
            let args = args
                .iter()
                .map(lower_current_lexical)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok((
                NativeExpr::TailCall {
                    function,
                    args,
                    yield_continuation_id: None,
                },
                Vec::new(),
            ));
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
        drop(owned_expr);
        return lower_prepared_call(
            region,
            YieldLoweringScope {
                param_names,
                params,
                param_types,
                param_core_types,
                completion,
            },
            &YieldLoweringEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                dynamic_profiles,
                module,
                function,
                arity,
                return_type,
            },
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        );
    }

    if let Some(region) = condition_yield_region(expr) {
        return lower_prepared_yield(
            &region,
            YieldLoweringScope {
                param_names,
                params,
                param_types,
                param_core_types,
                completion,
            },
            &YieldLoweringEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                dynamic_profiles,
                module,
                function,
                arity,
                return_type,
            },
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        );
    }

    if let CoreExpr::Let { bindings, body } = expr {
        if let Some(lowered) = lower_suspending_let_binding(
            bindings,
            body,
            YieldLoweringScope {
                param_names,
                params,
                param_types,
                param_core_types,
                completion,
            },
            environment,
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        )? {
            return Ok(lowered);
        }
    }

    if matches!(expr, CoreExpr::Case { .. }) {
        return super::structured_case::lower_suspending_case(
            expr,
            YieldLoweringScope {
                param_names,
                params,
                param_types,
                param_core_types,
                completion,
            },
            environment,
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        );
    }

    if let CoreExpr::Let { bindings, body } = expr {
        if super::contains_process_yield(body)
            || super::expr_calls_suspending(body, suspending_functions)
        {
            let mut entry_names = param_names.to_vec();
            let mut entry_vars = params.clone();
            let mut entry_types = param_types.clone();
            let mut entry_core_types = param_core_types.clone();
            let mut entry_bindings = Vec::with_capacity(bindings.len());
            let mut next_entry_local = next_local_index(&entry_vars);
            let retained = super::escape::retained_managed_bindings(bindings, body);
            for (binding, retained) in bindings.iter().zip(retained) {
                if !retained {
                    continue;
                }
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
                .or_else(|| {
                    super::structured_case::core_expr_type(
                        &binding.value,
                        &entry_core_types,
                        function_core_types,
                    )
                    .and_then(|core_type| {
                        super::native_type(Some(&core_type), &core_type.contract_text())
                    })
                })
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.control_prefix]: cannot infer native type for `{name}`"
                    )
                })?;
                let value = super::structured_case::lower_lexical_expr(
                    &binding.value,
                    &entry_vars,
                    &entry_types,
                    &entry_core_types,
                    super::structured_case::StructuredCaseEnvironment {
                        functions,
                        function_types,
                        function_core_types,
                        constructors,
                    },
                )?;
                entry_names.retain(|entry| entry != name);
                entry_names.push(name.clone());
                entry_vars.insert(name.clone(), next_entry_local);
                entry_types.insert(name.clone(), value_type);
                if let Some(core_type) = super::structured_case::core_expr_type(
                    &binding.value,
                    &entry_core_types,
                    function_core_types,
                ) {
                    entry_core_types.insert(name.clone(), core_type);
                }
                entry_bindings.push(value);
                next_entry_local = next_entry_local.saturating_add(1);
            }
            let (body, continuations) = lower_owned_expr_with_yields(
                body.as_ref().clone(),
                YieldLoweringScope {
                    param_names: &entry_names,
                    params: &entry_vars,
                    param_types: &entry_types,
                    param_core_types: &entry_core_types,
                    completion,
                },
                &YieldLoweringEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    dynamic_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                },
                &mut YieldLoweringState {
                    ordinal,
                    stable_ids,
                },
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
                result_core_type: condition_region.result_core_type,
                resume: CoreExpr::If {
                    clauses: resumed_clauses,
                },
                source_span: condition_region.source_span,
            };
            return lower_prepared_yield(
                &region,
                YieldLoweringScope {
                    param_names,
                    params,
                    param_types,
                    param_core_types,
                    completion,
                },
                &YieldLoweringEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    dynamic_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                },
                &mut YieldLoweringState {
                    ordinal,
                    stable_ids,
                },
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
                call_region,
                YieldLoweringScope {
                    param_names,
                    params,
                    param_types,
                    param_core_types,
                    completion,
                },
                &YieldLoweringEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    dynamic_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                },
                &mut YieldLoweringState {
                    ordinal,
                    stable_ids,
                },
            );
        }

        if let CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } = &first.condition
        {
            if matches!(operator.as_str(), "and" | "or") && !expr_is_scalar(&first.condition) {
                let left = lower_current_lexical(left)?;
                let mut right_clauses = Vec::with_capacity(clauses.len());
                right_clauses.push(CoreIfClause {
                    condition: right.as_ref().clone(),
                    body: first.body.clone(),
                });
                right_clauses.extend_from_slice(remaining);
                let (right_path, mut continuations) = lower_owned_expr_with_yields(
                    CoreExpr::If {
                        clauses: right_clauses,
                    },
                    YieldLoweringScope {
                        param_names,
                        params,
                        param_types,
                        param_core_types,
                        completion,
                    },
                    &YieldLoweringEnvironment {
                        functions,
                        function_types,
                        function_core_types,
                        constructors,
                        suspending_functions,
                        terminal_profiles,
                        dynamic_profiles,
                        module,
                        function,
                        arity,
                        return_type,
                    },
                    &mut YieldLoweringState {
                        ordinal,
                        stable_ids,
                    },
                )?;
                if operator == "and" {
                    let (fallback, fallback_continuations) = if remaining.is_empty() {
                        (
                            NativeExpr::If {
                                clauses: Vec::new(),
                            },
                            Vec::new(),
                        )
                    } else {
                        lower_owned_expr_with_yields(
                            CoreExpr::If {
                                clauses: remaining.to_vec(),
                            },
                            YieldLoweringScope {
                                param_names,
                                params,
                                param_types,
                                param_core_types,
                                completion,
                            },
                            &YieldLoweringEnvironment {
                                functions,
                                function_types,
                                function_core_types,
                                constructors,
                                suspending_functions,
                                terminal_profiles,
                                dynamic_profiles,
                                module,
                                function,
                                arity,
                                return_type,
                            },
                            &mut YieldLoweringState {
                                ordinal,
                                stable_ids,
                            },
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
                let (selected, selected_continuations) = lower_owned_expr_with_yields(
                    first.body.clone(),
                    YieldLoweringScope {
                        param_names,
                        params,
                        param_types,
                        param_core_types,
                        completion,
                    },
                    &YieldLoweringEnvironment {
                        functions,
                        function_types,
                        function_core_types,
                        constructors,
                        suspending_functions,
                        terminal_profiles,
                        dynamic_profiles,
                        module,
                        function,
                        arity,
                        return_type,
                    },
                    &mut YieldLoweringState {
                        ordinal,
                        stable_ids,
                    },
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

        let condition = lower_current_lexical(&first.condition)?;
        let (body, mut continuations) = lower_owned_expr_with_yields(
            first.body.clone(),
            YieldLoweringScope {
                param_names,
                params,
                param_types,
                param_core_types,
                completion,
            },
            &YieldLoweringEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                dynamic_profiles,
                module,
                function,
                arity,
                return_type,
            },
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        )?;
        let mut lowered_clauses = vec![(condition, body)];
        if !remaining.is_empty() {
            let (fallback, fallback_continuations) = lower_owned_expr_with_yields(
                CoreExpr::If {
                    clauses: remaining.to_vec(),
                },
                YieldLoweringScope {
                    param_names,
                    params,
                    param_types,
                    param_core_types,
                    completion,
                },
                &YieldLoweringEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    dynamic_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                },
                &mut YieldLoweringState {
                    ordinal,
                    stable_ids,
                },
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
        if matches!(operator.as_str(), "and" | "or") {
            if let Some(left_region) = yield_region(left) {
                let region = YieldRegion {
                    prefix: left_region.prefix,
                    operation: left_region.operation,
                    arguments: left_region.arguments,
                    result: left_region.result,
                    result_core_type: left_region.result_core_type,
                    resume: CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: Box::new(left_region.resume),
                        right: right.clone(),
                    },
                    source_span: left_region.source_span,
                };
                return lower_prepared_yield(
                    &region,
                    YieldLoweringScope {
                        param_names,
                        params,
                        param_types,
                        param_core_types,
                        completion,
                    },
                    &YieldLoweringEnvironment {
                        functions,
                        function_types,
                        function_core_types,
                        constructors,
                        suspending_functions,
                        terminal_profiles,
                        dynamic_profiles,
                        module,
                        function,
                        arity,
                        return_type,
                    },
                    &mut YieldLoweringState {
                        ordinal,
                        stable_ids,
                    },
                );
            }
            let left = lower_current_lexical(left)?;
            let (right, continuations) = lower_owned_expr_with_yields(
                right.as_ref().clone(),
                YieldLoweringScope {
                    param_names,
                    params,
                    param_types,
                    param_core_types,
                    completion,
                },
                &YieldLoweringEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                    suspending_functions,
                    terminal_profiles,
                    dynamic_profiles,
                    module,
                    function,
                    arity,
                    return_type,
                },
                &mut YieldLoweringState {
                    ordinal,
                    stable_ids,
                },
            )?;
            // Both outcomes must finish the surrounding binding before the
            // caller resumes. Returning the bypass Boolean directly loses a
            // pending join, including construction of a managed result.
            let bypass = complete(NativeExpr::Bool(operator == "or"), completion, params)?;
            let clauses = if operator == "and" {
                vec![(left, right), (NativeExpr::Bool(true), bypass)]
            } else {
                vec![(left, bypass), (NativeExpr::Bool(true), right)]
            };
            return Ok((NativeExpr::If { clauses }, continuations));
        }
    }

    if super::expr_calls_suspending(expr, suspending_functions)
        || super::contains_process_yield(expr)
    {
        return Err(format!(
            "error[native_ir.unlowered_suspension_context]: suspension-aware lowering has no evaluation context for {expr:#?}"
        ));
    }

    let expected_type = completion.map_or(return_type, |target| target.result_type);
    let declared_return = function_core_types
        .get(&(function.to_string(), arity))
        .or_else(|| function_core_types.get(&(format!("{module}.{function}"), arity)));
    let expected_core = match completion {
        Some(target) => target.result_core_type.as_ref(),
        None => declared_return,
    };
    let typed_value = expected_core
        .map(|expected_core| {
            super::collection_values::try_lower_typed_value(
                expr,
                expected_core,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
            .map_err(|error| {
                format!(
                    "{error}; while lowering typed aggregate result of `{module}.{function}/{arity}` as `{}`",
                    expected_core.contract_text()
                )
            })
        })
        .transpose()?
        .flatten();
    let value = if let Some(value) = typed_value {
        value
    } else if let Some(value) =
        super::constructors::lower_zero_field_managed_variant(expr, expected_type, constructors)?
    {
        value
    } else {
        match (expr, expected_type) {
            (CoreExpr::List(items), NativeType::ManagedRef(semantic)) if items.is_empty() => {
                NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_list_empty_operation(semantic)),
                    args: Vec::new(),
                }
            }
            (CoreExpr::ListCons { head, tail }, NativeType::ManagedRef(semantic)) => {
                NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_list_prepend_operation(semantic)),
                    args: vec![lower_current_lexical(head)?, lower_current_lexical(tail)?],
                }
            }
            _ => super::structured_case::lower_lexical_expr(
                expr,
                params,
                param_types,
                param_core_types,
                super::structured_case::StructuredCaseEnvironment {
                    functions,
                    function_types,
                    function_core_types,
                    constructors,
                },
            )?,
        }
    };
    Ok((complete(value, completion, params)?, Vec::new()))
}
#[path = "control/prepared_call.rs"]
mod prepared_call;
use prepared_call::*;

mod gated_call;
mod let_binding;
mod yield_lowering;

use gated_call::lower_gated_prepared_call;
use let_binding::lower_suspending_let_binding;
use yield_lowering::lower_prepared_yield;
