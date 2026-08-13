use super::*;

pub(super) fn lower_prepared_call(
    mut region: CallRegion,
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
    if !region.gates.is_empty() {
        return lower_gated_prepared_call(
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
    // The composed region may introduce a closure alias before invoking it
    // (`let saved = callback; saved(value)`). Recover that checked lexical
    // type before classifying the dynamic call target.
    let mut call_core_types = param_core_types.clone();
    for binding in &region.prefix {
        let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
            continue;
        };
        if let Some(core_type) = super::super::structured_case::core_expr_type(
            &binding.value,
            &call_core_types,
            function_core_types,
        ) {
            call_core_types.insert(name.clone(), core_type);
        }
    }
    let (function_index, dynamic_call, profiles, result_type, result_core_type, call_label) =
        match &region.target {
            CallTarget::Direct(target) => {
                let identity = (target.clone(), region.args.len());
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
                let result_type = function_types.get(&identity).copied().ok_or_else(|| {
                    format!(
                        "error[native_ir.call_type]: composed call `{}/{}` has no native return type",
                        identity.0, identity.1
                    )
                })?;
                (
                    Some(function_index),
                    None,
                    vec![(None, profile)],
                    result_type,
                    function_core_types.get(&identity).cloned(),
                    format!("`{}/{}`", identity.0, identity.1),
                )
            }
            CallTarget::Dynamic(callee) => {
                let core_type = super::super::structured_case::core_expr_type(
                    callee,
                    &call_core_types,
                    function_core_types,
                )
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.dynamic_call_type]: cannot infer closure type for `{}`",
                        callee.contract_text()
                    )
                })?;
                let CoreType::Arrow {
                    params: parameter_core_types,
                    return_type,
                } = core_type
                else {
                    return Err(format!(
                        "error[native_ir.dynamic_call_type]: `{}` is not a closure",
                        callee.contract_text()
                    ));
                };
                let parameter_types = parameter_core_types
                    .iter()
                    .map(|ty| {
                        super::super::native_type(Some(ty), &ty.contract_text()).ok_or_else(|| {
                            "error[native_ir.dynamic_call_type]: closure parameter is not native"
                                .to_string()
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let result_type = super::super::native_type(
                    Some(return_type.as_ref()),
                    &return_type.contract_text(),
                )
                .ok_or_else(|| {
                    "error[native_ir.dynamic_call_type]: closure result is not native".to_string()
                })?;
                let signature = DynamicCallSignature {
                    parameters: parameter_types.clone(),
                    result: result_type,
                };
                let targets = dynamic_profiles.get(&signature).ok_or_else(|| {
                    format!(
                        "error[native_ir.dynamic_call_profile]: closure `{}` has no suspension profile",
                        callee.contract_text()
                    )
                })?;
                let profiles = targets
                    .iter()
                    .map(|target| (Some(target.export_id), &target.profile))
                    .collect::<Vec<_>>();
                (
                    None,
                    Some((callee.clone(), parameter_types)),
                    profiles,
                    result_type,
                    Some(return_type.as_ref().clone()),
                    format!("dynamic `{}`", callee.contract_text()),
                )
            }
        };
    let call_ordinal = *ordinal;
    *ordinal = ordinal.saturating_add(1);
    let completion_id = stable_composed_completion_id(module, function, arity, call_ordinal);
    if !stable_ids.insert(completion_id) {
        return Err(format!(
            "error[native_ir.continuation_id_collision]: continuation id {completion_id} collides in module `{module}`"
        ));
    }

    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_core_types = param_core_types.clone();
    let mut entry_bindings = Vec::with_capacity(region.prefix.len());
    let mut next_entry_local = next_local_index(&entry_vars);
    for binding in &region.prefix {
        let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.call_prefix]: composed call prefix requires scalar variable bindings"
                    .to_string(),
            );
        };
        let value_type =
            lexical_result_type(&binding.value, &entry_types, &entry_core_types, environment)
                .ok_or_else(|| {
                    format!(
                "error[native_ir.call_prefix]: cannot infer scalar prefix `{name}` from {:#?}",
                binding.value
            )
                })?;
        let value = super::super::structured_case::lower_lexical_expr(
            &binding.value,
            &entry_vars,
            &entry_types,
            &entry_core_types,
            super::super::structured_case::StructuredCaseEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
            },
        )?;
        entry_bindings.push(value);
        entry_vars.insert(name.clone(), next_entry_local);
        entry_types.insert(name.clone(), value_type);
        if let Some(core_type) = super::super::structured_case::core_expr_type(
            &binding.value,
            &entry_core_types,
            function_core_types,
        ) {
            entry_core_types.insert(name.clone(), core_type);
        }
        next_entry_local = next_entry_local.saturating_add(1);
    }

    let mut captures = free_variables(&region.resume);
    captures.remove(&region.result_name);
    if let Some(completion) = completion {
        captures.extend(completion.captures.iter().cloned());
    }
    let mut visible_names = entry_vars
        .iter()
        .map(|(name, slot)| (*slot, name.clone()))
        .collect::<Vec<_>>();
    visible_names.sort_by_key(|(slot, _)| *slot);
    let capture_names = visible_names
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| captures.contains(name))
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
    let mut resume_core_types = capture_names
        .iter()
        .filter_map(|name| {
            entry_core_types
                .get(name)
                .cloned()
                .map(|core_type| (name.clone(), core_type))
        })
        .collect::<HashMap<_, _>>();
    if let Some(result_core_type) = result_core_type {
        resume_core_types.insert(region.result_name.clone(), result_core_type);
    }
    let resume = std::mem::replace(
        &mut region.resume,
        CoreExpr::Atom("$native_moved_resume".to_string()),
    );
    let (resumed_caller, nested_continuations) = lower_owned_expr_with_yields(
        resume,
        YieldLoweringScope {
            param_names: &resume_names,
            params: &resume_vars,
            param_types: &resume_types,
            param_core_types: &resume_core_types,
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

    // The callee owns its continuation graph.  A non-tail caller contributes
    // one completion frame, not one wrapper function per callee node.  The VM
    // retains this frame while the original callee continuation is parked and
    // enters `completion_id` only after that callee completes.
    let mut continuations = Vec::with_capacity(1 + nested_continuations.len());
    let mut completion_params = capture_types;
    completion_params.push(result_type);
    continuations.push(NativeContinuation {
        id: completion_id,
        source_module: module.to_string(),
        source_function: function.to_string(),
        source_arity: arity,
        source_span: None,
        capture_names,
        params: completion_params,
        return_type,
        body: resumed_caller,
    });
    continuations.extend(nested_continuations);
    let mut direct_resumes = Vec::new();
    let mut dynamic_resumes = Vec::new();
    for (target, profile) in &profiles {
        let profile_by_id = profile
            .continuations
            .iter()
            .map(|continuation| (continuation.id, continuation))
            .collect::<HashMap<_, _>>();
        for entry in &profile.entries {
            let callee = profile_by_id.get(entry).copied().ok_or_else(|| {
                format!(
                    "error[native_ir.call_then]: composed call {call_label} has an absent entry continuation {entry}"
                )
            })?;
            if callee.completion_result {
                return Err(format!(
                    "error[native_ir.call_then]: composed call {call_label} enters through a completion continuation"
                ));
            }
            if let Some(callee_export_id) = target {
                dynamic_resumes.push(super::super::NativeDynamicCallResume {
                    callee_export_id: *callee_export_id,
                    callee_continuation_id: callee.id,
                    callee_capture_count: callee.params.len(),
                    continuation_id: completion_id,
                });
            } else {
                direct_resumes.push(super::super::NativeCallResume {
                    callee_continuation_id: callee.id,
                    callee_capture_count: callee.params.len(),
                    continuation_id: completion_id,
                    caller_value_start: 0,
                });
            }
        }
    }
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
    let call = if let Some(function) = function_index {
        NativeExpr::CallThen {
            function,
            args,
            resumes: direct_resumes,
            completion_continuation_id: completion_id,
            completion_function: None,
            values,
        }
    } else {
        let (callee, parameter_types) =
            dynamic_call.expect("dynamic metadata accompanies the dynamic target");
        let callee = super::super::structured_case::lower_lexical_expr(
            &callee,
            &entry_vars,
            &entry_types,
            &entry_core_types,
            super::super::structured_case::StructuredCaseEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
            },
        )?;
        NativeExpr::InvokeClosureThen {
            callee: Box::new(callee),
            args,
            parameter_types,
            result_type,
            resumes: dynamic_resumes,
            completion_continuation_id: completion_id,
            completion_function: None,
            values,
        }
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
        continuations,
    ))
}
