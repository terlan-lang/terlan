//! Suspension-aware backedges and terminal calls for NativeIR tail calls.

use cranelift_codegen::ir::{
    condcodes::IntCC, types, BlockArg, InstBuilder, MemFlagsData, StackSlot, Value,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_object::ObjectModule;

use super::super::{NativeExpr, NativeTransitionOperation};
use super::emit_expr;
use super::function::{component_lane_widths, pack_component_values};
use super::setup::declare_image_func_in_func;
use super::transition::transition_status;
use super::{NativeFunctionCatalog, NativeTailFrame, NativeTransitionFrame};
use crate::runtime::native_image::managed::MANAGED_CONTEXT_COLLECTION_REQUESTED_OFFSET;

/// One terminal NativeIR call and its optional scheduler-yield continuation.
pub(super) struct SuspendingTailCall<'a> {
    pub(super) function: usize,
    pub(super) arguments: &'a [NativeExpr],
    pub(super) yield_continuation_id: Option<u64>,
}

pub(super) fn emit_suspending_tail_call(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    call: SuspendingTailCall<'_>,
    params: &[Value],
    catalog: NativeFunctionCatalog<'_>,
    tail: NativeTailFrame<'_>,
    transition: NativeTransitionFrame,
) -> Result<(), String> {
    let SuspendingTailCall {
        function,
        arguments,
        yield_continuation_id,
    } = call;
    let NativeFunctionCatalog {
        ids: function_ids,
        parameter_types,
        suspending: function_suspending,
        transition_counts: function_transition_counts,
        managed_layouts,
        ..
    } = catalog;
    let NativeTailFrame {
        self_function,
        component: tail_component,
        loop_header,
        reduction_budget_slot,
        managed_pressure,
        error_block,
    } = tail;
    let NativeTransitionFrame {
        pointer: transition_pointer,
        len_pointer: transition_len_pointer,
    } = transition;
    let function_id = function_ids.get(function).copied().ok_or_else(|| {
        format!("error[cranelift.tail_call]: native function {function} is unavailable")
    })?;
    let argument_values = arguments
        .iter()
        .map(|argument| {
            emit_expr(
                builder,
                module,
                argument,
                params,
                function_ids,
                managed_layouts,
                error_block,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(continuation_id) = yield_continuation_id {
        emit_budget_boundary(
            builder,
            &argument_values,
            continuation_id,
            NativeTransitionFrame {
                pointer: transition_pointer,
                len_pointer: transition_len_pointer,
            },
            reduction_budget_slot,
            params[0],
            managed_pressure,
        )?;
    }
    let component_target = tail_component
        .and_then(|component| {
            component
                .iter()
                .find(|(candidate, _)| *candidate == function)
        })
        .copied();
    if let Some((_, target_arity)) = component_target {
        if argument_values.len() != target_arity {
            return Err(format!(
                "error[cranelift.tail_component_arity]: function {function} expects {target_arity} argument(s), found {}",
                argument_values.len()
            ));
        }
    }
    let mut args = if component_target.is_some() {
        let component = tail_component.unwrap_or_default();
        let (managed_width, scalar_width) = component_lane_widths(component, parameter_types)?;
        let target_types = parameter_types.get(function).ok_or_else(|| {
            format!("error[cranelift.tail_component]: parameter types for function {function} are unavailable")
        })?;
        pack_component_values(
            builder,
            argument_values,
            target_types,
            managed_width,
            scalar_width,
        )?
    } else {
        argument_values
    };
    args.splice(
        0..0,
        params[..super::RUNTIME_ARGUMENT_COUNT].iter().copied(),
    );
    if function_transition_counts
        .get(function)
        .copied()
        .unwrap_or(0)
        > 0
    {
        args.push(transition_pointer.ok_or_else(|| {
            "error[cranelift.tail_call]: transition buffer is unavailable".to_string()
        })?);
    }
    if function_suspending.get(function).copied().unwrap_or(false) {
        args.push(transition_len_pointer.ok_or_else(|| {
            "error[cranelift.tail_call]: transition length output is unavailable".to_string()
        })?);
    }
    if component_target.is_some() {
        args.push(builder.ins().iconst(types::I64, function as i64));
    }
    if component_target.is_some() || self_function == Some(function) {
        if args.len() != builder.block_params(loop_header).len() {
            return Err(format!(
                "error[cranelift.tail_loop_arity]: suspending self tail call has {} ABI argument(s), expected {}",
                args.len(),
                builder.block_params(loop_header).len()
            ));
        }
        let args = args.into_iter().map(BlockArg::Value).collect::<Vec<_>>();
        builder.ins().jump(loop_header, &args);
        return Ok(());
    }
    let function_ref = declare_image_func_in_func(module, function_id, builder.func);
    let call = builder.ins().call(function_ref, &args);
    let results = builder.inst_results(call).to_vec();
    builder.ins().return_(&results);
    Ok(())
}

fn emit_budget_boundary(
    builder: &mut FunctionBuilder<'_>,
    arguments: &[Value],
    continuation_id: u64,
    transition: NativeTransitionFrame,
    reduction_budget_slot: Option<StackSlot>,
    runtime_context: Value,
    managed_pressure: bool,
) -> Result<(), String> {
    let NativeTransitionFrame {
        pointer: transition_pointer,
        len_pointer: transition_len_pointer,
    } = transition;
    let budget_slot = reduction_budget_slot.ok_or_else(|| {
        "error[cranelift.reduction_budget]: recursive backedge has no reduction budget".to_string()
    })?;
    let current = builder.ins().stack_load(types::I64, budget_slot, 0);
    let remaining = builder.ins().iadd_imm(current, -1);
    let exhausted = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);
    let should_yield = if managed_pressure {
        let requested = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            runtime_context,
            MANAGED_CONTEXT_COLLECTION_REQUESTED_OFFSET,
        );
        let requested = builder.ins().icmp_imm(IntCC::NotEqual, requested, 0);
        builder.ins().bor(exhausted, requested)
    } else {
        exhausted
    };
    let yield_block = builder.create_block();
    let continue_block = builder.create_block();
    builder
        .ins()
        .brif(should_yield, yield_block, &[], continue_block, &[]);

    builder.switch_to_block(yield_block);
    if !arguments.is_empty() {
        let pointer = transition_pointer.ok_or_else(|| {
            "error[cranelift.reduction_transition]: recursive yield has no transition buffer"
                .to_string()
        })?;
        for (index, argument) in arguments.iter().copied().enumerate() {
            let offset = i32::try_from(index.saturating_mul(8)).map_err(|_| {
                "error[cranelift.reduction_transition]: argument offset exceeds i32".to_string()
            })?;
            builder
                .ins()
                .store(MemFlagsData::new(), argument, pointer, offset);
        }
    }
    let len_pointer = transition_len_pointer.ok_or_else(|| {
        "error[cranelift.reduction_transition]: recursive yield has no length output".to_string()
    })?;
    let value_count = builder.ins().iconst(types::I64, arguments.len() as i64);
    builder
        .ins()
        .store(MemFlagsData::new(), value_count, len_pointer, 0);
    let status = builder.ins().iconst(
        types::I32,
        i64::from(transition_status(NativeTransitionOperation::Yield)),
    );
    let continuation = builder.ins().iconst(types::I64, continuation_id as i64);
    builder.ins().return_(&[status, continuation]);

    builder.switch_to_block(continue_block);
    builder.ins().stack_store(remaining, budget_slot, 0);
    Ok(())
}
