// Shared-join lowering for short-circuited suspending calls.

#[allow(clippy::too_many_arguments)]
fn lower_gated_prepared_call(
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
    let gate = region
        .gates
        .first()
        .cloned()
        .expect("gated call lowering requires a gate");
    let mut entry_names = param_names.to_vec();
    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_bindings = Vec::with_capacity(region.prefix.len());
    let mut next_entry_local = next_local_index(&entry_vars);
    for binding in &region.prefix {
        let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.call_gate_prefix]: call gate prefix requires variable bindings"
                    .to_string(),
            );
        };
        let value_type = super::infer_native_type_with_constructors(
            &binding.value,
            &entry_types,
            function_types,
            constructors,
        )
        .ok_or_else(|| {
            format!("error[native_ir.call_gate_prefix]: cannot infer prefix `{name}`")
        })?;
        let value = lower_expr_with_constructors(
            &binding.value,
            &entry_vars,
            &entry_types,
            functions,
            function_types,
            constructors,
        )?;
        entry_names.push(name.clone());
        entry_vars.insert(name.clone(), next_entry_local);
        entry_types.insert(name.clone(), value_type);
        entry_bindings.push(value);
        next_entry_local = next_entry_local.saturating_add(1);
    }
    let condition = lower_expr_with_constructors(
        &gate.condition,
        &entry_vars,
        &entry_types,
        functions,
        function_types,
        constructors,
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
        if let Some(call_result_type) = function_types
            .get(&(region.function.clone(), region.args.len()))
            .copied()
        {
            branch_types.insert(region.result_name.clone(), call_result_type);
        }
        let called_result_type = super::infer_native_type_with_constructors(
            &region.resume,
            &branch_types,
            function_types,
            constructors,
        );
        let bypass_result_type = super::infer_native_type_with_constructors(
            &gate.bypass_resume,
            &branch_types,
            function_types,
            constructors,
        );
        if let (Some(called), Some(bypass)) = (called_result_type, bypass_result_type) {
            if called != bypass {
                return Err(format!(
                    "error[native_ir.shared_completion_type]: gated branches disagree: {called:?} versus {bypass:?}"
                ));
            }
        }
        let result_type = called_result_type.or(bypass_result_type).ok_or_else(|| {
            "error[native_ir.shared_completion_type]: cannot infer gated result type".to_string()
        })?;
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
        resume_types.insert(join.result_name, result_type);
        let (body, nested) = lower_owned_expr_with_yields(
            join.resume,
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
        let mut params = capture_types;
        params.push(result_type);
        shared_continuations.push(NativeContinuation {
            id: continuation_id,
            source_module: module.to_string(),
            source_function: function.to_string(),
            source_arity: arity,
            params,
            return_type,
            body,
        });
        shared_continuations.extend(nested);
        Some(CompletionTarget {
            continuation_id,
            captures: capture_names,
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
        branch_completion,
    )?;
    let (bypass, bypass_continuations) = lower_owned_expr_with_yields(
        gate.bypass_resume.clone(),
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
        branch_completion,
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
