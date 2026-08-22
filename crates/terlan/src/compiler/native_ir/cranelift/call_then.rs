//! Synchronous side of a suspension-aware native call.

use cranelift_codegen::ir::{types, Block, BlockArg, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;
use cranelift_object::ObjectModule;

use super::super::NativeExpr;
use super::function::{component_lane_widths, pack_component_values};
use super::setup::declare_image_func_in_func;
use super::{NativeFunctionCatalog, NativeTailFrame, NativeTransitionFrame};

/// Synchronous result and continuation expression of one suspension-aware call.
pub(super) struct SynchronousCompletion<'a> {
    pub(super) function: Option<usize>,
    pub(super) values: &'a [NativeExpr],
    pub(super) call_value: Value,
}

/// Caller control-flow destinations shared by synchronous completion paths.
pub(super) struct SynchronousControl<'a> {
    pub(super) tail: NativeTailFrame<'a>,
    pub(super) error_block: Block,
}

/// Enters the caller-owned completion function when a callee did not park.
pub(super) fn return_from_synchronous_completion(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    completion: SynchronousCompletion<'_>,
    params: &[Value],
    transition: NativeTransitionFrame,
    catalog: NativeFunctionCatalog<'_>,
    control: SynchronousControl<'_>,
) -> Result<(), String> {
    let SynchronousControl { tail, error_block } = control;
    let SynchronousCompletion {
        function: completion_function,
        values,
        call_value,
    } = completion;
    let NativeTransitionFrame {
        pointer: transition_pointer,
        len_pointer: transition_len_pointer,
    } = transition;
    let NativeFunctionCatalog {
        ids: function_ids,
        parameter_types,
        suspending: function_suspending,
        transition_counts: function_transition_counts,
        managed_layouts,
        ..
    } = catalog;
    let completion_function = completion_function.ok_or_else(|| {
        "error[cranelift.call_then]: synchronous completion target is unresolved".to_string()
    })?;
    let component_target = tail
        .component
        .and_then(|component| {
            component
                .iter()
                .find(|(candidate, _)| *candidate == completion_function)
        })
        .copied();
    let completion_id = function_ids
        .get(completion_function)
        .copied()
        .ok_or_else(|| {
            format!(
                "error[cranelift.call_then]: completion function {completion_function} is unavailable"
            )
        })?;
    let mut completion_args = values
        .iter()
        .map(|value| {
            super::emit_expr(
                builder,
                module,
                value,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    completion_args.push(call_value);
    if let Some((_, target_arity)) = component_target {
        if completion_args.len() != target_arity {
            return Err(format!(
                "error[cranelift.tail_component_arity]: completion function {completion_function} expects {target_arity} argument(s), found {}",
                completion_args.len()
            ));
        }
        let component = tail.component.unwrap_or_default();
        let (managed_width, scalar_width) = component_lane_widths(component, parameter_types)?;
        let target_types = parameter_types.get(completion_function).ok_or_else(|| {
            format!("error[cranelift.tail_component]: parameter types for completion function {completion_function} are unavailable")
        })?;
        completion_args = pack_component_values(
            builder,
            completion_args,
            target_types,
            managed_width,
            scalar_width,
        )?;
    }
    completion_args.splice(
        0..0,
        params[..super::RUNTIME_ARGUMENT_COUNT].iter().copied(),
    );
    if function_transition_counts
        .get(completion_function)
        .copied()
        .unwrap_or(0)
        > 0
    {
        completion_args.push(transition_pointer.ok_or_else(|| {
            "error[cranelift.call_then]: completion transition buffer is unavailable".to_string()
        })?);
    }
    if function_suspending
        .get(completion_function)
        .copied()
        .unwrap_or(false)
    {
        completion_args.push(transition_len_pointer.ok_or_else(|| {
            "error[cranelift.call_then]: completion transition length output is unavailable"
                .to_string()
        })?);
    }
    if component_target.is_some() {
        completion_args.push(builder.ins().iconst(types::I64, completion_function as i64));
        if completion_args.len() != builder.block_params(tail.loop_header).len() {
            return Err(format!(
                "error[cranelift.tail_loop_arity]: synchronous completion has {} ABI argument(s), expected {}",
                completion_args.len(),
                builder.block_params(tail.loop_header).len()
            ));
        }
        let arguments = completion_args
            .into_iter()
            .map(BlockArg::Value)
            .collect::<Vec<_>>();
        builder.ins().jump(tail.loop_header, &arguments);
        return Ok(());
    }
    let completion_ref = declare_image_func_in_func(module, completion_id, builder.func);
    let completion_call = builder.ins().call(completion_ref, &completion_args);
    let completion_results = builder.inst_results(completion_call).to_vec();
    builder.ins().return_(&completion_results);
    Ok(())
}
