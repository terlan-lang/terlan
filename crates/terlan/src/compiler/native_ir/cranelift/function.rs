//! Definition of one local native function with its closed suspension ABI.

use cranelift_codegen::ir::{types, Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;

use super::super::{status, NativeExpr};
use super::managed::ManagedLayouts;
use super::suspension::{is_suspending, suspension_value_count};
use super::{emit_expr, emit_suspending_body};

#[allow(clippy::too_many_arguments)]
pub(super) fn define_native_function(
    module: &mut ObjectModule,
    function_id: FuncId,
    signature: &Signature,
    body: &NativeExpr,
    function_ids: &[FuncId],
    function_suspending: &[bool],
    function_transition_counts: &[usize],
    managed_layouts: &ManagedLayouts,
) -> Result<(), String> {
    let mut context = Context::new();
    context.func = Function::with_name_signature(
        UserFuncName::user(0, function_id.as_u32()),
        signature.clone(),
    );
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        let error = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.append_block_param(error, types::I32);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        if is_suspending(body, function_suspending) {
            let transition_value_count = suspension_value_count(body, function_transition_counts);
            let transition_len_pointer = params.last().copied();
            let source_end = params.len() - 1 - usize::from(transition_value_count > 0);
            let source_params = &params[..source_end];
            let transition_pointer = (transition_value_count > 0).then(|| params[source_end]);
            emit_suspending_body(
                &mut builder,
                module,
                body,
                source_params,
                transition_pointer,
                transition_len_pointer,
                function_ids,
                function_suspending,
                function_transition_counts,
                managed_layouts,
                error,
            )?;
        } else {
            let value = emit_expr(
                &mut builder,
                module,
                body,
                &params,
                function_ids,
                managed_layouts,
                error,
            )?;
            let ok = builder.ins().iconst(types::I32, i64::from(status::OK));
            builder.ins().return_(&[ok, value]);
        }
        builder.switch_to_block(error);
        let error_status = builder.block_params(error)[0];
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[error_status, zero]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .map_err(|error| {
            format!(
                "error[cranelift.define]: function {}: {error}: {error:?}",
                function_id.as_u32()
            )
        })
}
