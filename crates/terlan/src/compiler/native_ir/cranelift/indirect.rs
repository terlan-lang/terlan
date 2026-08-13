//! Owned-closure resolution and image-local indirect dispatch emission.

use cranelift_codegen::ir::{
    types, AbiParam, Block, BlockArg, InstBuilder, Signature, StackSlot, StackSlotData,
    StackSlotKind, Value,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;

use super::super::{status, NativeType, DISPATCH_SYMBOL};
use super::dispatch::dispatch_signature;
use super::setup::declare_image_func_in_func;
use crate::runtime::native_image::TVM_INDIRECT_TRANSITION_WORD_CAPACITY;

const MAX_INVOCATION_WORDS: usize = 128;

/// VM-owned values forwarded through one generated indirect invocation.
pub(super) struct IndirectRuntimeValues {
    pub(super) context: Value,
    pub(super) allocator: Value,
    pub(super) resolver: Value,
    pub(super) lookup: Value,
}

/// Statically known closure-call shape plus its emitted operands.
pub(super) struct IndirectInvocation<'a> {
    pub(super) closure: Value,
    pub(super) arguments: &'a [Value],
    pub(super) parameter_types: &'a [NativeType],
    pub(super) result_type: NativeType,
}

/// Optional caller-owned transition storage for a suspending invocation.
#[derive(Clone, Copy)]
pub(super) struct IndirectTransition {
    pub(super) pointer: Option<Value>,
    pub(super) len_pointer: Option<Value>,
}

/// Evaluates one closure operand followed by its ordered caller arguments.
pub(super) fn emit_operands(
    callee: &super::super::NativeExpr,
    arguments: &[super::super::NativeExpr],
    mut emit: impl FnMut(&super::super::NativeExpr) -> Result<Value, String>,
) -> Result<(Value, Vec<Value>), String> {
    let callee = emit(callee)?;
    let arguments = arguments.iter().map(emit).collect::<Result<Vec<_>, _>>()?;
    Ok((callee, arguments))
}

/// Invokes a closure that is statically proven not to suspend.
pub(super) fn emit_invoke_closure(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    runtime: IndirectRuntimeValues,
    invocation: IndirectInvocation<'_>,
    error_block: Block,
) -> Result<Value, String> {
    let (dispatch_status, value, _) = emit_invoke_closure_raw(
        builder,
        module,
        runtime,
        invocation,
        IndirectTransition {
            pointer: None,
            len_pointer: None,
        },
        error_block,
    )?;
    branch_status(builder, dispatch_status, error_block);
    Ok(value)
}

/// Invokes a closure while forwarding the caller's bounded transition frame.
pub(super) fn emit_suspending_invoke_closure(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    runtime: IndirectRuntimeValues,
    invocation: IndirectInvocation<'_>,
    transition: IndirectTransition,
    error_block: Block,
) -> Result<(Value, Value, Value), String> {
    emit_invoke_closure_raw(
        builder,
        module,
        runtime,
        invocation,
        transition,
        error_block,
    )
}

/// Validates an actor-owned closure and recursively enters the sealed dispatcher.
fn emit_invoke_closure_raw(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    runtime: IndirectRuntimeValues,
    invocation: IndirectInvocation<'_>,
    transition: IndirectTransition,
    error_block: Block,
) -> Result<(Value, Value, Value), String> {
    let IndirectRuntimeValues {
        context: runtime_context,
        allocator,
        resolver,
        lookup,
    } = runtime;
    let IndirectInvocation {
        closure,
        arguments,
        parameter_types,
        result_type,
    } = invocation;
    if arguments.len() != parameter_types.len() {
        return Err("error[cranelift.closure_arity]: inconsistent indirect call shape".into());
    }
    let pointer = module.target_config().pointer_type();
    let resolver_missing =
        builder
            .ins()
            .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, resolver, 0);
    let missing = builder.create_block();
    let resolver_ready = builder.create_block();
    builder
        .ins()
        .brif(resolver_missing, missing, &[], resolver_ready, &[]);
    builder.switch_to_block(missing);
    let unavailable = builder
        .ins()
        .iconst(types::I32, i64::from(status::MANAGED_RUNTIME_UNAVAILABLE));
    builder
        .ins()
        .jump(error_block, &[BlockArg::Value(unavailable)]);
    builder.switch_to_block(resolver_ready);

    let parameter_type_slot = type_words_slot(builder, parameter_types)?;
    let result_type_slot = type_words_slot(builder, &[result_type])?;
    let argument_slot = words_slot(builder, arguments, "closure arguments")?;
    let invocation_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        (MAX_INVOCATION_WORDS * 8) as u32,
        3,
    ));
    let target_slot = scalar_slot(builder);
    let invocation_len_slot = scalar_slot(builder);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().stack_store(zero, target_slot, 0);
    builder.ins().stack_store(zero, invocation_len_slot, 0);

    let resolver_signature = Signature {
        params: vec![
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv: module.target_config().default_call_conv,
    };
    let resolver_signature = builder.import_signature(resolver_signature);
    let parameter_count = builder.ins().iconst(types::I64, arguments.len() as i64);
    let result_count = builder.ins().iconst(types::I64, 1);
    let invocation_capacity = builder
        .ins()
        .iconst(types::I64, MAX_INVOCATION_WORDS as i64);
    let parameter_types_pointer = builder.ins().stack_addr(pointer, parameter_type_slot, 0);
    let arguments_pointer = builder.ins().stack_addr(pointer, argument_slot, 0);
    let result_types_pointer = builder.ins().stack_addr(pointer, result_type_slot, 0);
    let target_pointer = builder.ins().stack_addr(pointer, target_slot, 0);
    let invocation_pointer = builder.ins().stack_addr(pointer, invocation_slot, 0);
    let invocation_len_pointer = builder.ins().stack_addr(pointer, invocation_len_slot, 0);
    let resolver_call = builder.ins().call_indirect(
        resolver_signature,
        resolver,
        &[
            runtime_context,
            closure,
            parameter_types_pointer,
            parameter_count,
            arguments_pointer,
            result_types_pointer,
            result_count,
            target_pointer,
            invocation_pointer,
            invocation_capacity,
            invocation_len_pointer,
        ],
    );
    let resolver_status = builder.inst_results(resolver_call)[0];
    branch_status(builder, resolver_status, error_block);

    let signature = dispatch_signature(module);
    let dispatch_id = module
        .declare_function(DISPATCH_SYMBOL, Linkage::Import, &signature)
        .map_err(|error| format!("error[cranelift.closure_dispatch_declare]: {error}"))?;
    let dispatch_ref = declare_image_func_in_func(module, dispatch_id, builder.func);
    let result_slot = scalar_slot(builder);
    builder.ins().stack_store(zero, result_slot, 0);
    let target = builder.ins().stack_load(types::I64, target_slot, 0);
    let invocation_len = builder.ins().stack_load(types::I64, invocation_len_slot, 0);
    let result_pointer = builder.ins().stack_addr(pointer, result_slot, 0);
    let null = builder.ins().iconst(pointer, 0);
    let has_transition_storage = transition.pointer.is_some();
    let transition_pointer = transition.pointer.unwrap_or(null);
    let transition_capacity = builder.ins().iconst(
        types::I64,
        if has_transition_storage {
            TVM_INDIRECT_TRANSITION_WORD_CAPACITY as i64
        } else {
            0
        },
    );
    let transition_len_pointer = if let Some(pointer) = transition.len_pointer {
        pointer
    } else {
        let slot = scalar_slot(builder);
        builder.ins().stack_store(zero, slot, 0);
        builder.ins().stack_addr(pointer, slot, 0)
    };
    let dispatch_call = builder.ins().call(
        dispatch_ref,
        &[
            runtime_context,
            allocator,
            resolver,
            lookup,
            target,
            invocation_pointer,
            invocation_len,
            result_pointer,
            transition_pointer,
            transition_capacity,
            transition_len_pointer,
        ],
    );
    let dispatch_status = builder.inst_results(dispatch_call)[0];
    let result = builder.ins().stack_load(types::I64, result_slot, 0);
    Ok((dispatch_status, result, target))
}

/// Stores canonical three-word boundary identities in generated stack memory.
fn type_words_slot(
    builder: &mut FunctionBuilder<'_>,
    native_types: &[NativeType],
) -> Result<StackSlot, String> {
    let mut words = Vec::with_capacity(native_types.len().saturating_mul(3));
    for native_type in native_types {
        for word in native_type.boundary_type().transition_words() {
            words.push(builder.ins().iconst(types::I64, word));
        }
    }
    words_slot(builder, &words, "closure type identities")
}

/// Allocates one bounded stack word array.
fn words_slot(
    builder: &mut FunctionBuilder<'_>,
    words: &[Value],
    label: &str,
) -> Result<StackSlot, String> {
    let bytes = u32::try_from(words.len().saturating_mul(8).max(8))
        .map_err(|_| format!("error[cranelift.closure_buffer]: {label} exceed u32"))?;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, bytes, 3));
    for (index, word) in words.iter().enumerate() {
        let offset = i32::try_from(index.saturating_mul(8))
            .map_err(|_| format!("error[cranelift.closure_buffer]: {label} offset exceeds i32"))?;
        builder.ins().stack_store(*word, slot, offset);
    }
    Ok(slot)
}

fn scalar_slot(builder: &mut FunctionBuilder<'_>) -> StackSlot {
    builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3))
}

/// Forwards one nonzero callback/dispatch status into the function error block.
fn branch_status(builder: &mut FunctionBuilder<'_>, status: Value, error_block: Block) {
    let failed =
        builder
            .ins()
            .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::NotEqual, status, 0);
    let next = builder.create_block();
    builder
        .ins()
        .brif(failed, error_block, &[BlockArg::Value(status)], next, &[]);
    builder.switch_to_block(next);
}
