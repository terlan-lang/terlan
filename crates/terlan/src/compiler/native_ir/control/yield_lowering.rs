// Yield-region continuation construction kept separate from control orchestration.
use super::*;

pub(super) fn lower_prepared_yield(
    region: &YieldRegion,
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
    let continuation_id = stable_continuation_id(module, function, arity, *ordinal);
    *ordinal = ordinal.saturating_add(1);
    if !stable_ids.insert(continuation_id) {
        return Err(format!(
            "error[native_ir.continuation_id_collision]: continuation id {continuation_id} collides in module `{module}`"
        ));
    }
    let lowered = lower_yield_region(
        YieldRegionRequest {
            region,
            param_names,
            required_captures: completion.map_or(&[], |target| target.captures.as_slice()),
            continuation_id,
        },
        YieldRegionEnvironment {
            params,
            param_types,
            param_core_types,
            functions,
            function_types,
            function_core_types,
            constructors,
        },
    )?;
    let (resume_body, nested) = lower_owned_expr_with_yields(
        lowered.resume.clone(),
        YieldLoweringScope {
            param_names: &lowered.resume_names,
            params: &lowered.resume_vars,
            param_types: &lowered.resume_types,
            param_core_types: &lowered.resume_core_types,
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
    let mut continuations = Vec::with_capacity(nested.len() + 1);
    continuations.push(NativeContinuation {
        id: continuation_id,
        source_module: module.to_string(),
        source_function: function.to_string(),
        source_arity: arity,
        source_span: lowered.source_span,
        capture_names: lowered.capture_names,
        params: lowered.continuation_params,
        return_type,
        body: resume_body,
    });
    continuations.extend(nested);
    Ok((lowered.entry, continuations))
}
