//! Status-aware protected expression lowering for native `Try` regions.

use cranelift_codegen::ir::{types, Block, BlockArg, InstBuilder, Value};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::FuncId;
use cranelift_object::ObjectModule;

use super::super::NativeExpr;
use super::emit_expr;
use super::managed::ManagedLayouts;

/// Emits one protected expression, local failure handler, and exactly-once cleanup.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_try(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    protected: &NativeExpr,
    success: &NativeExpr,
    failure: &NativeExpr,
    cleanup: &[NativeExpr],
    params: &[Value],
    function_ids: &[FuncId],
    managed_layouts: &ManagedLayouts,
    outer_error: Block,
) -> Result<Value, String> {
    let caught = builder.create_block();
    let selected = builder.create_block();
    let cleanup_error = builder.create_block();
    let completed = builder.create_block();
    builder.append_block_param(caught, types::I32);
    builder.append_block_param(selected, types::I64);
    builder.append_block_param(cleanup_error, types::I32);
    builder.append_block_param(completed, types::I64);

    let protected_value = emit_expr(
        builder,
        module,
        protected,
        params,
        function_ids,
        managed_layouts,
        caught,
    )?;
    let mut success_params = params.to_vec();
    success_params.push(protected_value);
    let success_value = emit_expr(
        builder,
        module,
        success,
        &success_params,
        function_ids,
        managed_layouts,
        caught,
    )?;
    builder
        .ins()
        .jump(selected, &[BlockArg::Value(success_value)]);

    builder.switch_to_block(caught);
    let status = builder.block_params(caught)[0];
    let status_word = builder.ins().sextend(types::I64, status);
    let mut failure_params = params.to_vec();
    failure_params.push(status_word);
    let failure_value = emit_expr(
        builder,
        module,
        failure,
        &failure_params,
        function_ids,
        managed_layouts,
        cleanup_error,
    )?;
    builder
        .ins()
        .jump(selected, &[BlockArg::Value(failure_value)]);

    builder.switch_to_block(cleanup_error);
    let status = builder.block_params(cleanup_error)[0];
    emit_cleanup(
        builder,
        module,
        cleanup,
        params,
        function_ids,
        managed_layouts,
        outer_error,
    )?;
    builder.ins().jump(outer_error, &[BlockArg::Value(status)]);

    builder.switch_to_block(selected);
    let result = builder.block_params(selected)[0];
    emit_cleanup(
        builder,
        module,
        cleanup,
        params,
        function_ids,
        managed_layouts,
        outer_error,
    )?;
    builder.ins().jump(completed, &[BlockArg::Value(result)]);
    builder.switch_to_block(completed);
    Ok(builder.block_params(completed)[0])
}

#[allow(clippy::too_many_arguments)]
fn emit_cleanup(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    cleanup: &[NativeExpr],
    params: &[Value],
    function_ids: &[FuncId],
    managed_layouts: &ManagedLayouts,
    outer_error: Block,
) -> Result<(), String> {
    for expression in cleanup {
        let _ = emit_expr(
            builder,
            module,
            expression,
            params,
            function_ids,
            managed_layouts,
            outer_error,
        )?;
    }
    Ok(())
}
