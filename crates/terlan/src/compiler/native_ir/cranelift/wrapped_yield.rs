//! Completion-frame emission for suspension-aware calls.

use super::*;

/// Complete caller-owned state needed to wrap one callee suspension.
pub(super) struct WrappedCallYield<'a> {
    pub(super) call_status: Value,
    pub(super) call_value: Value,
    pub(super) transition: NativeTransitionFrame,
    pub(super) flags: &'a transition::TransitionFlags,
    pub(super) callee_capture_count: usize,
    pub(super) continuation_id: u64,
    pub(super) forward_callee_frame: bool,
    pub(super) caller_value_start: usize,
    pub(super) values: &'a [NativeExpr],
    pub(super) params: &'a [Value],
    pub(super) function_ids: &'a [FuncId],
    pub(super) managed_layouts: &'a ManagedLayouts,
    pub(super) error_block: Block,
}

pub(super) fn emit_wrapped_call_yield(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    yield_state: WrappedCallYield<'_>,
) -> Result<(), String> {
    let WrappedCallYield {
        call_status,
        call_value,
        transition:
            NativeTransitionFrame {
                pointer: transition_pointer,
                len_pointer: Some(len_pointer),
            },
        flags,
        callee_capture_count,
        continuation_id,
        forward_callee_frame,
        caller_value_start,
        values,
        params,
        function_ids,
        managed_layouts,
        error_block,
    } = yield_state
    else {
        return Err(
            "error[cranelift.call_then]: transition length output is unavailable".to_string(),
        );
    };
    let actual_count = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), len_pointer, 0);
    let expected_count =
        transition::expected_value_count(builder, flags, actual_count, callee_capture_count);
    let unexpected_count =
        builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, actual_count, expected_count);
    branch_on_flag(
        builder,
        unexpected_count,
        status::TRANSITION_CAPACITY,
        error_block,
    );
    if forward_callee_frame {
        // A recursive tail-component edge already owns the complete parked
        // frame under this same continuation identity. Appending a second
        // zero-width completion would change its declared capture shape.
        builder.ins().return_(&[call_status, call_value]);
        return Ok(());
    }
    let values = values.get(caller_value_start..).ok_or_else(|| {
        format!(
            "error[cranelift.call_then]: caller value offset {caller_value_start} exceeds frame width {}",
            values.len()
        )
    })?;
    let pointer = transition_pointer.ok_or_else(|| {
        "error[cranelift.call_then]: transition buffer is unavailable".to_string()
    })?;
    let byte_offset = builder.ins().imul_imm(actual_count, 8);
    let append_pointer = builder.ins().iadd(pointer, byte_offset);
    for (index, value) in values.iter().enumerate() {
        let captured = emit_expr(
            builder,
            module,
            value,
            params,
            function_ids,
            managed_layouts,
            error_block,
        )?;
        let offset = i32::try_from(index.saturating_mul(8))
            .map_err(|_| "error[cranelift.call_then]: transition offset exceeds i32".to_string())?;
        builder
            .ins()
            .store(MemFlagsData::new(), captured, append_pointer, offset);
    }
    let completion_offset = i32::try_from(values.len().saturating_mul(8)).map_err(|_| {
        "error[cranelift.call_then]: completion-frame offset exceeds i32".to_string()
    })?;
    let count_offset = completion_offset.checked_add(8).ok_or_else(|| {
        "error[cranelift.call_then]: completion-frame offset exceeds i32".to_string()
    })?;
    let completion = builder.ins().iconst(types::I64, continuation_id as i64);
    builder.ins().store(
        MemFlagsData::new(),
        completion,
        append_pointer,
        completion_offset,
    );
    let capture_count = builder.ins().iconst(types::I64, values.len() as i64);
    builder.ins().store(
        MemFlagsData::new(),
        capture_count,
        append_pointer,
        count_offset,
    );
    let value_count = builder
        .ins()
        .iadd_imm(actual_count, values.len().saturating_add(2) as i64);
    builder
        .ins()
        .store(MemFlagsData::new(), value_count, len_pointer, 0);
    builder.ins().return_(&[call_status, call_value]);
    Ok(())
}
