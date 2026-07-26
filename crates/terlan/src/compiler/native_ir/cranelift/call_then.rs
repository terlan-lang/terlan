//! Synchronous side of a suspension-aware native call.

use cranelift_codegen::ir::{Block, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;

use super::super::NativeExpr;
use super::managed::ManagedLayouts;

/// Enters the caller-owned completion function when a callee did not park.
#[allow(clippy::too_many_arguments)]
pub(super) fn return_from_synchronous_completion(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    completion_function: Option<usize>,
    values: &[NativeExpr],
    call_value: Value,
    params: &[Value],
    transition_pointer: Option<Value>,
    transition_len_pointer: Value,
    function_ids: &[FuncId],
    function_suspending: &[bool],
    function_transition_counts: &[usize],
    managed_layouts: &ManagedLayouts,
    error_block: Block,
) -> Result<(), String> {
    let completion_function = completion_function.ok_or_else(|| {
        "error[cranelift.call_then]: synchronous completion target is unresolved".to_string()
    })?;
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
    completion_args.splice(0..0, [params[0], params[1], params[2]]);
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
        completion_args.push(transition_len_pointer);
    }
    let completion_ref = module.declare_func_in_func(completion_id, builder.func);
    let completion_call = builder.ins().call(completion_ref, &completion_args);
    let completion_results = builder.inst_results(completion_call).to_vec();
    builder.ins().return_(&completion_results);
    Ok(())
}
