//! Suspension-aware control lowering for Terlan NativeIR.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_list_empty_operation, encode_list_prepend_operation,
};
use crate::terlan_typeck::{CoreExpr, CoreIfClause};

use super::{
    composed_call_region,
    composed_continuation::wrap_composed_continuation,
    condition_yield_region,
    constructors::NativeConstructorLayouts,
    control_completion::{complete, CompletionTarget},
    expr_is_scalar, free_variables, lower_expr_with_constructors, lower_yield_region,
    rebase_callee_locals, stable_continuation_id, yield_region, CallRegion, ComposedCallProfile,
    NativeContinuation, NativeExpr, NativeType, YieldRegion,
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
    lower_owned_expr_with_yields(
        expr.clone(),
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
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_owned_expr_with_yields(
    owned_expr: CoreExpr,
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
    completion: Option<&CompletionTarget>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    let expr = &owned_expr;
    if let CoreExpr::Call { function, args } = expr {
        let identity = (function.clone(), args.len());
        if suspending_functions.contains(&identity) && completion.is_none() {
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
        drop(owned_expr);
        return lower_prepared_call(
            region,
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
            completion,
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
            completion,
        );
    }

    if let CoreExpr::Let { bindings, body } = expr {
        if super::contains_process_yield(body) {
            let mut entry_names = param_names.to_vec();
            let mut entry_vars = params.clone();
            let mut entry_types = param_types.clone();
            let mut entry_bindings = Vec::with_capacity(bindings.len());
            let mut next_entry_local = next_local_index(&entry_vars);
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
                entry_names.retain(|entry| entry != name);
                entry_names.push(name.clone());
                entry_vars.insert(name.clone(), next_entry_local);
                entry_types.insert(name.clone(), value_type);
                entry_bindings.push(value);
                next_entry_local = next_entry_local.saturating_add(1);
            }
            let (body, continuations) = lower_owned_expr_with_yields(
                body.as_ref().clone(),
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
                completion,
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
                completion,
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
                completion,
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
                let (right_path, mut continuations) = lower_owned_expr_with_yields(
                    CoreExpr::If {
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
                    completion,
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
                        lower_owned_expr_with_yields(
                            CoreExpr::If {
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
                            completion,
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
                    completion,
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
        let (body, mut continuations) = lower_owned_expr_with_yields(
            first.body.clone(),
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
            completion,
        )?;
        let mut lowered_clauses = vec![(condition, body)];
        if !remaining.is_empty() {
            let (fallback, fallback_continuations) = lower_owned_expr_with_yields(
                CoreExpr::If {
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
                completion,
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
                    completion,
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
            let (right, continuations) = lower_owned_expr_with_yields(
                right.as_ref().clone(),
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
                completion,
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

    let value = match (expr, return_type) {
        (CoreExpr::List(items), NativeType::ManagedRef(semantic)) if items.is_empty() => {
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_empty_operation(semantic)),
                args: Vec::new(),
            }
        }
        (CoreExpr::ListCons { head, tail }, NativeType::ManagedRef(semantic)) => {
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_prepend_operation(semantic)),
                args: vec![
                    lower_expr_with_constructors(
                        head,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?,
                    lower_expr_with_constructors(
                        tail,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?,
                ],
            }
        }
        _ => lower_expr_with_constructors(
            expr,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?,
    };
    Ok((complete(value, completion, params)?, Vec::new()))
}

#[allow(clippy::too_many_arguments)]
fn lower_prepared_call(
    mut region: CallRegion,
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
    completion: Option<&CompletionTarget>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    if !region.gates.is_empty() {
        return lower_gated_prepared_call(
            region,
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
            completion,
        );
    }
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
    let mut wrapper_ids = HashMap::with_capacity(profile.continuations.len());
    for callee in &profile.continuations {
        let continuation_id = stable_continuation_id(module, function, arity, *ordinal);
        *ordinal = ordinal.saturating_add(1);
        if !stable_ids.insert(continuation_id) {
            return Err(format!(
                "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{module}`"
            ));
        }
        wrapper_ids.insert(callee.id, continuation_id);
    }
    let completion_id = stable_continuation_id(module, function, arity, *ordinal);
    *ordinal = ordinal.saturating_add(1);
    if !stable_ids.insert(completion_id) {
        return Err(format!(
            "error[native_ir.continuation_id_collision]: continuation id {completion_id} collides in module `{module}`"
        ));
    }

    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_bindings = Vec::with_capacity(region.prefix.len());
    let mut prefix_names = Vec::with_capacity(region.prefix.len());
    let mut next_entry_local = next_local_index(&entry_vars);
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
        entry_vars.insert(name.clone(), next_entry_local);
        entry_types.insert(name.clone(), value_type);
        prefix_names.push(name.clone());
        next_entry_local = next_entry_local.saturating_add(1);
    }

    let mut captures = free_variables(&region.resume);
    captures.remove(&region.result_name);
    if let Some(completion) = completion {
        captures.extend(completion.captures.iter().cloned());
    }
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
    let mut resume_names = capture_names.clone();
    resume_names.push(region.result_name.clone());
    let mut resume_vars = capture_names
        .iter()
        .enumerate()
        .map(|(capture_index, name)| (name.clone(), capture_index))
        .collect::<HashMap<_, _>>();
    resume_vars.insert(region.result_name.clone(), capture_names.len());
    let mut resume_types = capture_names
        .iter()
        .zip(capture_types.iter().copied())
        .map(|(name, ty)| (name.clone(), ty))
        .collect::<HashMap<_, _>>();
    resume_types.insert(region.result_name.clone(), result_type);
    let resume = std::mem::replace(
        &mut region.resume,
        CoreExpr::Atom("$native_moved_resume".to_string()),
    );
    let (resumed_caller, nested_continuations) = lower_owned_expr_with_yields(
        resume,
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
        ordinal,
        stable_ids,
        completion,
    )?;

    let mut wrappers = Vec::with_capacity(
        profile
            .continuations
            .len()
            .saturating_add(1)
            .saturating_add(nested_continuations.len()),
    );
    for callee in &profile.continuations {
        let callee_capture_count = callee.params.len();
        let mut continuation_params = callee.params.clone();
        continuation_params.extend(capture_types.iter().copied());
        let rebased_body =
            rebase_callee_locals(&callee.body, callee_capture_count, capture_names.len());
        let body = wrap_composed_continuation(
            &rebased_body,
            callee_capture_count,
            capture_names.len(),
            &wrapper_ids,
            completion_id,
        )?;
        wrappers.push(NativeContinuation {
            id: wrapper_ids[&callee.id],
            source_module: module.to_string(),
            source_function: function.to_string(),
            source_arity: arity,
            params: continuation_params,
            return_type,
            body,
        });
    }
    let mut completion_params = capture_types;
    completion_params.push(result_type);
    wrappers.push(NativeContinuation {
        id: completion_id,
        source_module: module.to_string(),
        source_function: function.to_string(),
        source_arity: arity,
        params: completion_params,
        return_type,
        body: resumed_caller,
    });
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
        continuation_id: wrapper_ids[&first_callee.id],
        completion_continuation_id: completion_id,
        completion_function: None,
        values,
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
include!("control_gated.rs");

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
    completion: Option<&CompletionTarget>,
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
    let (resume_body, nested) = lower_owned_expr_with_yields(
        lowered.resume.clone(),
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
        completion,
    )?;
    let mut continuations = Vec::with_capacity(nested.len() + 1);
    continuations.push(NativeContinuation {
        id: continuation_id,
        source_module: module.to_string(),
        source_function: function.to_string(),
        source_arity: arity,
        params: lowered.continuation_params,
        return_type,
        body: resume_body,
    });
    continuations.extend(nested);
    Ok((lowered.entry, continuations))
}
