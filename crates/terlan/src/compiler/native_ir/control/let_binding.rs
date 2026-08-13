//! Join-continuation lowering for suspending lexical binding values.

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};

use super::*;

/// Lowers the first suspending let binding into a stable join continuation.
///
/// Pure prefix bindings become entry locals, live lexical values become
/// continuation captures, and the remaining bindings and body resume with the
/// completed value bound under its source name.
pub(super) fn lower_suspending_let_binding(
    bindings: &[CoreLetBinding],
    body: &CoreExpr,
    scope: YieldLoweringScope<'_>,
    environment: &YieldLoweringEnvironment<'_>,
    state: &mut YieldLoweringState<'_>,
) -> Result<Option<(NativeExpr, Vec<NativeContinuation>)>, super::super::NativeIrError> {
    let Some(binding_index) = bindings.iter().position(|binding| {
        expr_calls_suspending(&binding.value, environment.suspending_functions)
            || contains_process_yield(&binding.value)
    }) else {
        return Ok(None);
    };
    let YieldLoweringScope {
        param_names,
        params,
        param_types,
        param_core_types,
        completion,
    } = scope;
    let mut entry_names = param_names.to_vec();
    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_core_types = param_core_types.clone();
    let mut entry_bindings = Vec::with_capacity(binding_index);
    let mut next_entry_local = next_local_index(&entry_vars);
    for binding in &bindings[..binding_index] {
        let CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.let_prefix]: suspending let prefix requires variable bindings"
                    .to_string()
                    .into(),
            );
        };
        let value_type =
            lexical_result_type(&binding.value, &entry_types, &entry_core_types, environment)
                .ok_or_else(|| {
                    format!("error[native_ir.let_prefix]: cannot infer native type for `{name}`")
                })?;
        let value = super::super::structured_case::lower_lexical_expr(
            &binding.value,
            &entry_vars,
            &entry_types,
            &entry_core_types,
            super::super::structured_case::StructuredCaseEnvironment {
                functions: environment.functions,
                function_types: environment.function_types,
                function_core_types: environment.function_core_types,
                constructors: environment.constructors,
            },
        )?;
        entry_names.retain(|entry| entry != name);
        entry_names.push(name.clone());
        entry_vars.insert(name.clone(), next_entry_local);
        entry_types.insert(name.clone(), value_type);
        if let Some(core_type) = super::super::structured_case::core_expr_type(
            &binding.value,
            &entry_core_types,
            environment.function_core_types,
        ) {
            entry_core_types.insert(name.clone(), core_type);
        }
        entry_bindings.push(value);
        next_entry_local = next_entry_local.saturating_add(1);
    }

    let binding = &bindings[binding_index];
    let CorePattern::Var(result_name) = &binding.pattern else {
        return Err(
            "error[native_ir.let_result]: suspending let value requires a variable binding"
                .to_string()
                .into(),
        );
    };
    let result_core_type = super::super::structured_case::core_expr_type(
        &binding.value,
        &entry_core_types,
        environment.function_core_types,
    );
    let result_type = lexical_result_type(
        &binding.value,
        &entry_types,
        &entry_core_types,
        environment,
    )
    .ok_or_else(|| {
        format!(
            "error[native_ir.let_result]: cannot infer native type for suspending binding `{result_name}`"
        )
    })?;
    let resume = if binding_index + 1 == bindings.len() {
        body.clone()
    } else {
        CoreExpr::Let {
            bindings: bindings[binding_index + 1..].to_vec(),
            body: Box::new(body.clone()),
        }
    };
    let mut captures = free_variables(&resume);
    captures.remove(result_name);
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
            "error[native_ir.let_capture]: continuation references unavailable scalar `{unknown}`"
        )
        .into());
    }
    let capture_types = capture_names
        .iter()
        .map(|name| {
            entry_types.get(name).copied().ok_or_else(|| {
                format!("error[native_ir.let_capture]: scalar `{name}` has no native type")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let continuation_id = stable_continuation_id(
        environment.module,
        environment.function,
        environment.arity,
        *state.ordinal,
    );
    *state.ordinal = state.ordinal.saturating_add(1);
    if !state.stable_ids.insert(continuation_id) {
        return Err(format!(
            "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{}`",
            environment.module
        )
        .into());
    }

    let mut resume_names = capture_names.clone();
    resume_names.push(result_name.clone());
    let mut resume_vars = capture_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<HashMap<_, _>>();
    resume_vars.insert(result_name.clone(), capture_names.len());
    let mut resume_types = capture_names
        .iter()
        .zip(capture_types.iter().copied())
        .map(|(name, ty)| (name.clone(), ty))
        .collect::<HashMap<_, _>>();
    resume_types.insert(result_name.clone(), result_type);
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
        resume_core_types.insert(result_name.clone(), result_core_type.clone());
    }
    let (resume_body, mut continuations) = lower_owned_expr_with_yields(
        resume,
        YieldLoweringScope {
            param_names: &resume_names,
            params: &resume_vars,
            param_types: &resume_types,
            param_core_types: &resume_core_types,
            completion,
        },
        environment,
        state,
    )?;
    let mut continuation_params = capture_types;
    continuation_params.push(result_type);
    continuations.push(NativeContinuation {
        id: continuation_id,
        source_module: environment.module.to_string(),
        source_function: environment.function.to_string(),
        source_arity: environment.arity,
        source_span: None,
        capture_names: capture_names.clone(),
        params: continuation_params,
        return_type: environment.return_type,
        body: resume_body,
    });

    let target = CompletionTarget {
        continuation_id,
        captures: capture_names,
        result_type,
        result_core_type,
    };
    let (binding_body, binding_continuations) = lower_owned_expr_with_yields(
        binding.value.clone(),
        YieldLoweringScope {
            param_names: &entry_names,
            params: &entry_vars,
            param_types: &entry_types,
            param_core_types: &entry_core_types,
            completion: Some(&target),
        },
        environment,
        state,
    )?;
    continuations.extend(binding_continuations);
    let entry = if entry_bindings.is_empty() {
        binding_body
    } else {
        NativeExpr::Let {
            bindings: entry_bindings,
            body: Box::new(binding_body),
        }
    };
    Ok(Some((entry, continuations)))
}
