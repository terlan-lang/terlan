// Shared-join lowering for short-circuited suspending calls.
use super::*;

pub(super) fn lower_gated_prepared_call(
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
    let gate = region
        .gates
        .first()
        .cloned()
        .expect("gated call lowering requires a gate");
    let mut entry_names = param_names.to_vec();
    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_core_types = param_core_types.clone();
    let mut entry_bindings = Vec::with_capacity(region.prefix.len());
    let mut next_entry_local = next_local_index(&entry_vars);
    for binding in &region.prefix {
        let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.call_gate_prefix]: call gate prefix requires variable bindings"
                    .to_string(),
            );
        };
        let value_type =
            lexical_result_type(&binding.value, &entry_types, &entry_core_types, environment)
                .ok_or_else(|| {
                    format!("error[native_ir.call_gate_prefix]: cannot infer prefix `{name}`")
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
        entry_names.push(name.clone());
        entry_vars.insert(name.clone(), next_entry_local);
        entry_types.insert(name.clone(), value_type);
        if let Some(core_type) = super::super::structured_case::core_expr_type(
            &binding.value,
            &entry_core_types,
            function_core_types,
        ) {
            entry_core_types.insert(name.clone(), core_type);
        }
        entry_bindings.push(value);
        next_entry_local = next_entry_local.saturating_add(1);
    }
    let condition = super::super::structured_case::lower_lexical_expr(
        &gate.condition,
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
    let mut shared_continuations = Vec::new();
    let shared_target = if let Some(join) = region.join.take() {
        let continuation_id = stable_continuation_id(module, function, arity, *ordinal);
        *ordinal = ordinal.saturating_add(1);
        if !stable_ids.insert(continuation_id) {
            return Err(format!(
                "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{module}`"
            ));
        }
        let mut captures = free_variables(&join.resume);
        captures.remove(&join.result_name);
        if let Some(completion) = completion {
            captures.extend(completion.captures.iter().cloned());
        }
        let capture_names = entry_names
            .iter()
            .filter(|name| captures.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(unknown) = captures.iter().find(|name| !entry_vars.contains_key(*name)) {
            return Err(format!(
                "error[native_ir.shared_completion_capture]: scalar `{unknown}` is unavailable"
            ));
        }
        let capture_types = capture_names
            .iter()
            .map(|name| {
                entry_types.get(name).copied().ok_or_else(|| {
                    format!(
                        "error[native_ir.shared_completion_type]: scalar `{name}` has no native type"
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut branch_types = entry_types.clone();
        let call_result_core_type = match &region.target {
            CallTarget::Direct(function) => function_core_types
                .get(&(function.clone(), region.args.len()))
                .cloned(),
            CallTarget::Dynamic(callee) => super::super::structured_case::core_expr_type(
                callee,
                &entry_core_types,
                function_core_types,
            )
            .and_then(|ty| match ty {
                CoreType::Arrow { return_type, .. } => Some(return_type.as_ref().clone()),
                _ => None,
            }),
        };
        let call_result_type = call_result_core_type
            .as_ref()
            .and_then(|ty| super::super::native_type(Some(ty), &ty.contract_text()))
            .or_else(|| match &region.target {
                CallTarget::Direct(function) => function_types
                    .get(&(function.clone(), region.args.len()))
                    .copied(),
                CallTarget::Dynamic(_) => None,
            });
        if let Some(call_result_type) = call_result_type {
            branch_types.insert(region.result_name.clone(), call_result_type);
        }
        let mut branch_core_types = entry_core_types.clone();
        if let Some(call_result_core_type) = call_result_core_type {
            branch_core_types.insert(region.result_name.clone(), call_result_core_type);
        }
        // Ask the semantic control-type recovery to reconcile the two arms.
        // In particular, an atom-form `None` inherits the concrete managed
        // `Option[T]` representation selected by a `Some(value)` sibling.
        let called_result_core_type = super::super::structured_case::core_expr_type(
            &region.resume,
            &branch_core_types,
            function_core_types,
        )
        .or_else(|| {
            super::super::constructors::constructor_result_core_type(&region.resume, constructors)
        });
        let bypass_result_core_type = super::super::structured_case::core_expr_type(
            &gate.bypass_resume,
            &branch_core_types,
            function_core_types,
        )
        .or_else(|| {
            super::super::constructors::constructor_result_core_type(
                &gate.bypass_resume,
                constructors,
            )
        });
        let result_core_type = match (
            called_result_core_type.as_ref(),
            bypass_result_core_type.as_ref(),
        ) {
            (Some(called), Some(bypass)) if called == bypass => Some(called.clone()),
            (Some(called), None)
                if super::super::collection_values::is_none_option_value(
                    &gate.bypass_resume,
                    called,
                ) =>
            {
                Some(called.clone())
            }
            (None, Some(bypass))
                if super::super::collection_values::is_none_option_value(
                    &region.resume,
                    bypass,
                ) =>
            {
                Some(bypass.clone())
            }
            _ => super::super::structured_case::core_expr_type(
                &CoreExpr::If {
                    clauses: vec![
                        CoreIfClause {
                            condition: CoreExpr::Atom("true".to_string()),
                            body: region.resume.clone(),
                        },
                        CoreIfClause {
                            condition: CoreExpr::Atom("true".to_string()),
                            body: gate.bypass_resume.clone(),
                        },
                    ],
                },
                &branch_core_types,
                function_core_types,
            ),
        };
        let called_result_type = super::super::infer_native_type_with_constructors(
            &region.resume,
            &branch_types,
            function_types,
            constructors,
        );
        let bypass_result_type = super::super::infer_native_type_with_constructors(
            &gate.bypass_resume,
            &branch_types,
            function_types,
            constructors,
        );
        let typed_result = result_core_type
            .as_ref()
            .and_then(|ty| super::super::native_type(Some(ty), &ty.contract_text()));
        let result_type = if let Some(typed_result) = typed_result {
            typed_result
        } else {
            if let (Some(called), Some(bypass)) = (called_result_type, bypass_result_type) {
                if called != bypass {
                    return Err(format!(
                        "error[native_ir.shared_completion_type]: gated branches disagree: {called:?} versus {bypass:?}"
                    ));
                }
            }
            called_result_type.or(bypass_result_type).ok_or_else(|| {
                format!(
                    "error[native_ir.shared_completion_type]: cannot infer gated result type (called_core={called_result_core_type:?}, bypass_core={bypass_result_core_type:?})"
                )
            })?
        };
        let mut resume_names = capture_names.clone();
        resume_names.push(join.result_name.clone());
        let mut resume_vars = capture_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect::<HashMap<_, _>>();
        resume_vars.insert(join.result_name.clone(), capture_names.len());
        let mut resume_types = capture_names
            .iter()
            .zip(capture_types.iter().copied())
            .map(|(name, ty)| (name.clone(), ty))
            .collect::<HashMap<_, _>>();
        resume_types.insert(join.result_name.clone(), result_type);
        let mut resume_core_types = capture_names
            .iter()
            .filter_map(|name| {
                entry_core_types
                    .get(name)
                    .cloned()
                    .map(|core_type| (name.clone(), core_type))
            })
            .collect::<HashMap<_, _>>();
        if let Some(result_core_type) = &result_core_type {
            resume_core_types.insert(join.result_name.clone(), result_core_type.clone());
        }
        let (body, nested) = lower_owned_expr_with_yields(
            join.resume,
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
        let mut params = capture_types;
        params.push(result_type);
        shared_continuations.push(NativeContinuation {
            id: continuation_id,
            source_module: module.to_string(),
            source_function: function.to_string(),
            source_arity: arity,
            source_span: None,
            capture_names: capture_names.clone(),
            params,
            return_type,
            body,
        });
        shared_continuations.extend(nested);
        Some(CompletionTarget {
            continuation_id,
            captures: capture_names,
            result_type,
            result_core_type,
        })
    } else {
        None
    };
    let branch_completion = shared_target.as_ref().or(completion);
    let mut called_region = region.clone();
    called_region.prefix = gate.prefix.clone();
    called_region.gates.remove(0);
    let (called, mut continuations) = lower_prepared_call(
        called_region,
        YieldLoweringScope {
            param_names: &entry_names,
            params: &entry_vars,
            param_types: &entry_types,
            param_core_types: &entry_core_types,
            completion: branch_completion,
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
    let (bypass, bypass_continuations) = lower_owned_expr_with_yields(
        gate.bypass_resume.clone(),
        YieldLoweringScope {
            param_names: &entry_names,
            params: &entry_vars,
            param_types: &entry_types,
            param_core_types: &entry_core_types,
            completion: branch_completion,
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
    continuations.extend(bypass_continuations);
    continuations.extend(shared_continuations);
    let branches = if gate.call_when_true {
        vec![(condition, called), (NativeExpr::Bool(true), bypass)]
    } else {
        vec![(condition, bypass), (NativeExpr::Bool(true), called)]
    };
    let body = NativeExpr::If { clauses: branches };
    Ok((
        if entry_bindings.is_empty() {
            body
        } else {
            NativeExpr::Let {
                bindings: entry_bindings,
                body: Box::new(body),
            }
        },
        continuations,
    ))
}
