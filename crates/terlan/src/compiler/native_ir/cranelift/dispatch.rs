//! Exported direct-AOT dispatcher emission.

use std::collections::BTreeSet;

use cranelift_codegen::ir::{
    condcodes::IntCC, types, AbiParam, BlockArg, Endianness, Function, InstBuilder, MemFlagsData,
    Signature, UserFuncName, Value,
};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use crate::runtime::native_image::dispatch_lookup::{
    TVM_DISPATCH_INDEX_ENTRY_BYTES, TVM_DISPATCH_RECORD_ENTRY_BYTES,
    TVM_DISPATCH_RECORD_FUNCTION_POINTER_OFFSET, TVM_DISPATCH_RECORD_SHAPE_OFFSET,
    TVM_DISPATCH_RECORD_TRANSITION_COUNT_OFFSET,
};

use super::super::{status, DISPATCH_SYMBOL};
use super::setup::{declare_image_data_in_func, declare_image_func_in_func};
use super::signature::native_signature;
use super::transition::transition_flags;
use super::RUNTIME_ARGUMENT_COUNT;

const DISPATCH_INDEX_SYMBOL: &str = "terlan_native_dispatch_index_v2";
const DISPATCH_RECORDS_SYMBOL: &str = "terlan_native_dispatch_records_v2";
const RARE_DISPATCH_SYMBOL: &str = "terlan_native_dispatch_rare_v1";
const COMMON_DIRECT_ARITY_MAX: usize = 8;
type DispatchFunction = (u64, usize, FuncId, usize, bool);

#[derive(Clone, Copy)]
struct ShapeIndirectCall {
    shape: u32,
    function_pointer: Value,
    runtime_context: Value,
    managed_allocator: Value,
    closure_resolver: Value,
    lookup_callback: Value,
    args_pointer: Value,
    transition_pointer: Value,
    transition_len_pointer: Value,
}

/// Returns the frozen exported image-dispatch signature.
pub(super) fn dispatch_signature(module: &ObjectModule) -> Signature {
    let pointer = module.target_config().pointer_type();
    Signature {
        params: vec![
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I64),
            AbiParam::new(pointer),
        ],
        returns: vec![AbiParam::new(types::I32)],
        call_conv: module.target_config().default_call_conv,
    }
}

fn rare_dispatch_signature(module: &ObjectModule) -> Signature {
    let pointer = module.target_config().pointer_type();
    Signature {
        params: vec![
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(types::I32),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
        ],
        returns: vec![AbiParam::new(types::I32), AbiParam::new(types::I64)],
        call_conv: module.target_config().default_call_conv,
    }
}

/// Defines the stable exported dispatcher over all generated native entries.
///
/// Export inventories can contain tens of thousands of continuations. Keeping
/// one full call/status block per export made the dispatcher itself several
/// megabytes and put that code on every continuation-resume path. The emitted
/// dispatcher therefore performs an O(1) lookup in immutable image data and
/// shares one indirect call site per ABI shape (arity plus transition flags).
pub(super) fn define_dispatch(
    module: &mut ObjectModule,
    functions: &[DispatchFunction],
) -> super::super::NativeIrResult<()> {
    let (index_id, records_id, table_length) = define_dispatch_tables(module, functions)?;
    let shapes = functions
        .iter()
        .map(dispatch_shape)
        .collect::<BTreeSet<_>>();
    let (common_shapes, rare_shapes): (BTreeSet<_>, BTreeSet<_>) = shapes
        .into_iter()
        .partition(|shape| decode_dispatch_shape(*shape).0 <= COMMON_DIRECT_ARITY_MAX);
    let rare_dispatch_id = (!rare_shapes.is_empty())
        .then(|| define_rare_dispatch(module, &rare_shapes))
        .transpose()?;
    let signature = dispatch_signature(module);
    let dispatch_id = module
        .declare_function(DISPATCH_SYMBOL, Linkage::Export, &signature)
        .map_err(|error| format!("error[cranelift.dispatch_declare]: {error}"))?;
    let pointer = module.target_config().pointer_type();
    let mut context = Context::new();
    context.func =
        Function::with_name_signature(UserFuncName::user(0, dispatch_id.as_u32()), signature);
    let index_global = declare_image_data_in_func(module, index_id, &mut context.func);
    let records_global = declare_image_data_in_func(module, records_id, &mut context.func);
    let rare_dispatch_ref = rare_dispatch_id
        .map(|function_id| declare_image_func_in_func(module, function_id, &mut context.func));
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let runtime_context = params[0];
        let managed_allocator = params[1];
        let closure_resolver = params[2];
        let lookup_callback = params[3];
        let export_id = params[4];
        let args_pointer = params[5];
        let supplied_arity = params[6];
        let result_pointer = params[7];
        let transition_pointer = params[8];
        let transition_capacity = params[9];
        let transition_len_pointer = params[10];
        let zero = builder.ins().iconst(types::I64, 0);
        builder
            .ins()
            .store(MemFlagsData::new(), zero, transition_len_pointer, 0);

        let index_pointer = builder.ins().global_value(pointer, index_global);
        let records_pointer = builder.ins().global_value(pointer, records_global);
        let table_mask = i64::try_from(table_length - 1)
            .map_err(|_| "error[cranelift.dispatch]: table mask exceeds i64".to_string())?;
        let mask = builder.ins().iconst(types::I64, table_mask);
        let lookup_signature = Signature {
            params: vec![
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(types::I64),
                AbiParam::new(types::I64),
            ],
            returns: vec![AbiParam::new(pointer)],
            call_conv: module.target_config().default_call_conv,
        };
        let lookup_signature = builder.import_signature(lookup_signature);
        let lookup = builder.ins().call_indirect(
            lookup_signature,
            lookup_callback,
            &[index_pointer, records_pointer, mask, export_id],
        );
        let record = builder.inst_results(lookup)[0];
        let found_record = builder.ins().icmp_imm(IntCC::NotEqual, record, 0);
        let found = builder.create_block();
        let unknown = builder.create_block();
        builder.ins().brif(found_record, found, &[], unknown, &[]);

        builder.switch_to_block(found);
        let function_pointer = builder.ins().load(
            pointer,
            MemFlagsData::new(),
            record,
            i32::try_from(TVM_DISPATCH_RECORD_FUNCTION_POINTER_OFFSET)
                .expect("dispatch function-pointer offset fits i32"),
        );
        let shape = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            record,
            i32::try_from(TVM_DISPATCH_RECORD_SHAPE_OFFSET)
                .expect("dispatch shape offset fits i32"),
        );
        let expected_arity_i32 = builder.ins().ushr_imm(shape, 2);
        let expected_arity = builder.ins().uextend(types::I64, expected_arity_i32);
        let arity_matches = builder
            .ins()
            .icmp(IntCC::Equal, supplied_arity, expected_arity);
        let inspect_capacity = builder.create_block();
        let arity_error = builder.create_block();
        builder
            .ins()
            .brif(arity_matches, inspect_capacity, &[], arity_error, &[]);

        builder.switch_to_block(arity_error);
        let arity = builder.ins().iconst(types::I32, i64::from(status::ARITY));
        builder.ins().return_(&[arity]);

        builder.switch_to_block(inspect_capacity);
        let transition_count_i32 = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            record,
            i32::try_from(TVM_DISPATCH_RECORD_TRANSITION_COUNT_OFFSET)
                .expect("dispatch transition-count offset fits i32"),
        );
        let transition_count = builder.ins().uextend(types::I64, transition_count_i32);
        let capacity_sufficient = builder.ins().icmp(
            IntCC::UnsignedGreaterThanOrEqual,
            transition_capacity,
            transition_count,
        );
        let select_shape = builder.create_block();
        let capacity_error = builder.create_block();
        builder
            .ins()
            .brif(capacity_sufficient, select_shape, &[], capacity_error, &[]);

        builder.switch_to_block(capacity_error);
        let capacity = builder
            .ins()
            .iconst(types::I32, i64::from(status::TRANSITION_CAPACITY));
        builder.ins().return_(&[capacity]);

        let call_result = builder.create_block();
        builder.append_block_param(call_result, types::I32);
        builder.append_block_param(call_result, types::I64);
        let invalid_shape = builder.create_block();
        let rare_shape = rare_dispatch_ref.map(|_| builder.create_block());
        let call_blocks = common_shapes
            .iter()
            .map(|shape| (*shape, builder.create_block()))
            .collect::<Vec<_>>();

        builder.switch_to_block(select_shape);
        let mut shape_switch = Switch::new();
        for (shape, block) in &call_blocks {
            shape_switch.set_entry(u128::from(*shape), *block);
        }
        shape_switch.emit(&mut builder, shape, rare_shape.unwrap_or(invalid_shape));

        for (shape, call_block) in call_blocks {
            builder.switch_to_block(call_block);
            let (call_status, value) = emit_shape_indirect_call(
                &mut builder,
                pointer,
                ShapeIndirectCall {
                    shape,
                    function_pointer,
                    runtime_context,
                    managed_allocator,
                    closure_resolver,
                    lookup_callback,
                    args_pointer,
                    transition_pointer,
                    transition_len_pointer,
                },
            )?;
            builder.ins().jump(
                call_result,
                &[BlockArg::Value(call_status), BlockArg::Value(value)],
            );
        }

        if let (Some(rare_shape), Some(rare_dispatch_ref)) = (rare_shape, rare_dispatch_ref) {
            builder.switch_to_block(rare_shape);
            let call = builder.ins().call(
                rare_dispatch_ref,
                &[
                    runtime_context,
                    managed_allocator,
                    closure_resolver,
                    lookup_callback,
                    function_pointer,
                    shape,
                    args_pointer,
                    transition_pointer,
                    transition_len_pointer,
                ],
            );
            let results = builder.inst_results(call).to_vec();
            builder.ins().jump(
                call_result,
                &[BlockArg::Value(results[0]), BlockArg::Value(results[1])],
            );
        }

        builder.switch_to_block(call_result);
        let call_status = builder.block_params(call_result)[0];
        let value = builder.block_params(call_result)[1];
        let succeeded = builder
            .ins()
            .icmp_imm(IntCC::Equal, call_status, i64::from(status::OK));
        let store_success = builder.create_block();
        let inspect_yield = builder.create_block();
        builder
            .ins()
            .brif(succeeded, store_success, &[], inspect_yield, &[]);
        builder.switch_to_block(store_success);
        builder
            .ins()
            .store(MemFlagsData::new(), value, result_pointer, 0);
        builder.ins().return_(&[call_status]);

        builder.switch_to_block(inspect_yield);
        let transitioned = transition_flags(&mut builder, call_status).transitioned;
        let store_yield = builder.create_block();
        let return_status = builder.create_block();
        builder
            .ins()
            .brif(transitioned, store_yield, &[], return_status, &[]);
        builder.switch_to_block(store_yield);
        builder
            .ins()
            .store(MemFlagsData::new(), value, result_pointer, 0);
        builder.ins().return_(&[call_status]);
        builder.switch_to_block(return_status);
        builder.ins().return_(&[call_status]);

        builder.switch_to_block(invalid_shape);
        let invalid = builder
            .ins()
            .iconst(types::I32, i64::from(status::UNKNOWN_EXPORT));
        builder.ins().return_(&[invalid]);

        builder.switch_to_block(unknown);
        let unknown = builder
            .ins()
            .iconst(types::I32, i64::from(status::UNKNOWN_EXPORT));
        builder.ins().return_(&[unknown]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    Ok(module
        .define_function(dispatch_id, &mut context)
        .map_err(|error| format!("error[cranelift.dispatch_define]: {error}"))?)
}

/// Emits one shared cold dispatcher for uncommon large call shapes.
///
/// Common arities stay inline in the exported dispatcher. Every larger shape
/// reuses this out-of-line packed-argument entry, preventing uncommon source
/// signatures from inflating the common instruction-cache footprint.
fn define_rare_dispatch(
    module: &mut ObjectModule,
    shapes: &BTreeSet<u32>,
) -> super::super::NativeIrResult<FuncId> {
    let signature = rare_dispatch_signature(module);
    let function_id = module
        .declare_function(RARE_DISPATCH_SYMBOL, Linkage::Local, &signature)
        .map_err(|error| format!("error[cranelift.rare_dispatch_declare]: {error}"))?;
    let pointer = module.target_config().pointer_type();
    let mut context = Context::new();
    context.func =
        Function::with_name_signature(UserFuncName::user(0, function_id.as_u32()), signature);
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let runtime_context = params[0];
        let managed_allocator = params[1];
        let closure_resolver = params[2];
        let lookup_callback = params[3];
        let function_pointer = params[4];
        let shape = params[5];
        let args_pointer = params[6];
        let transition_pointer = params[7];
        let transition_len_pointer = params[8];
        let invalid_shape = builder.create_block();
        let call_blocks = shapes
            .iter()
            .map(|shape| (*shape, builder.create_block()))
            .collect::<Vec<_>>();
        let mut shape_switch = Switch::new();
        for (shape, block) in &call_blocks {
            shape_switch.set_entry(u128::from(*shape), *block);
        }
        shape_switch.emit(&mut builder, shape, invalid_shape);

        for (shape, call_block) in call_blocks {
            builder.switch_to_block(call_block);
            let (call_status, value) = emit_shape_indirect_call(
                &mut builder,
                pointer,
                ShapeIndirectCall {
                    shape,
                    function_pointer,
                    runtime_context,
                    managed_allocator,
                    closure_resolver,
                    lookup_callback,
                    args_pointer,
                    transition_pointer,
                    transition_len_pointer,
                },
            )?;
            builder.ins().return_(&[call_status, value]);
        }

        builder.switch_to_block(invalid_shape);
        let invalid = builder
            .ins()
            .iconst(types::I32, i64::from(status::UNKNOWN_EXPORT));
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().return_(&[invalid, zero]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .map_err(|error| format!("error[cranelift.rare_dispatch_define]: {error}"))?;
    Ok(function_id)
}

/// Emits the one indirect native call shared by dispatchers for an ABI shape.
fn emit_shape_indirect_call(
    builder: &mut FunctionBuilder<'_>,
    pointer: cranelift_codegen::ir::Type,
    call: ShapeIndirectCall,
) -> super::super::NativeIrResult<(Value, Value)> {
    let (function_arity, has_transitions, suspending) = decode_dispatch_shape(call.shape);
    let mut args = Vec::with_capacity(function_arity.saturating_add(RUNTIME_ARGUMENT_COUNT + 2));
    args.extend([
        call.runtime_context,
        call.managed_allocator,
        call.closure_resolver,
        call.lookup_callback,
    ]);
    for argument_index in 0..function_arity {
        let offset = i32::try_from(argument_index.saturating_mul(8))
            .map_err(|_| "error[cranelift.dispatch]: argument offset exceeds i32".to_string())?;
        args.push(
            builder
                .ins()
                .load(types::I64, MemFlagsData::new(), call.args_pointer, offset),
        );
    }
    if has_transitions {
        args.push(call.transition_pointer);
    }
    if suspending {
        args.push(call.transition_len_pointer);
    }
    let signature = native_signature(
        function_arity,
        suspending,
        usize::from(has_transitions),
        pointer,
    );
    let signature_ref = builder.import_signature(signature);
    let instruction = builder
        .ins()
        .call_indirect(signature_ref, call.function_pointer, &args);
    let results = builder.inst_results(instruction);
    Ok((results[0], results[1]))
}

fn define_dispatch_tables(
    module: &mut ObjectModule,
    functions: &[DispatchFunction],
) -> super::super::NativeIrResult<(cranelift_module::DataId, cranelift_module::DataId, usize)> {
    if module.target_config().pointer_type() != types::I64 {
        return Err(
            "error[cranelift.dispatch]: table dispatch requires a 64-bit target"
                .to_string()
                .into(),
        );
    }
    let minimum_slots = functions.len().saturating_mul(2).max(2);
    let table_length = minimum_slots
        .checked_next_power_of_two()
        .ok_or_else(|| "error[cranelift.dispatch]: dispatch table is too large".to_string())?;
    let index_byte_length = table_length
        .checked_mul(TVM_DISPATCH_INDEX_ENTRY_BYTES)
        .ok_or_else(|| {
            "error[cranelift.dispatch]: dispatch index byte size overflow".to_string()
        })?;
    let records_byte_length = functions
        .len()
        .checked_mul(TVM_DISPATCH_RECORD_ENTRY_BYTES)
        .ok_or_else(|| {
            "error[cranelift.dispatch]: dispatch records byte size overflow".to_string()
        })?;
    let mut slots = vec![None; table_length];
    for (index, function) in functions.iter().enumerate() {
        let mut slot = (function.0 as usize) & (table_length - 1);
        loop {
            match slots[slot] {
                None => {
                    slots[slot] = Some(index);
                    break;
                }
                Some(existing) if functions[existing].0 == function.0 => {
                    return Err(format!(
                        "error[cranelift.dispatch]: duplicate export id {}",
                        function.0
                    )
                    .into());
                }
                Some(_) => slot = (slot + 1) & (table_length - 1),
            }
        }
    }

    let mut index_bytes = vec![0_u8; index_byte_length];
    for (slot, function_index) in slots.into_iter().enumerate() {
        let Some(function_index) = function_index else {
            continue;
        };
        let base = slot * TVM_DISPATCH_INDEX_ENTRY_BYTES;
        let record_index_tag = function_index.checked_add(1).ok_or_else(|| {
            "error[cranelift.dispatch]: dispatch record index overflow".to_string()
        })?;
        write_u32(
            module.isa().endianness(),
            &mut index_bytes[base..base + TVM_DISPATCH_INDEX_ENTRY_BYTES],
            u32::try_from(record_index_tag).map_err(|_| {
                "error[cranelift.dispatch]: dispatch record index exceeds u32".to_string()
            })?,
        );
    }
    let mut index_data = DataDescription::new();
    index_data.define(index_bytes.into_boxed_slice());
    index_data.set_align(4);
    let index_id = module
        .declare_data(DISPATCH_INDEX_SYMBOL, Linkage::Local, false, false)
        .map_err(|error| format!("error[cranelift.dispatch_index_declare]: {error}"))?;
    module
        .define_data(index_id, &index_data)
        .map_err(|error| format!("error[cranelift.dispatch_index_define]: {error}"))?;

    let mut record_bytes = vec![0_u8; records_byte_length];
    let mut records_data = DataDescription::new();
    for (record_index, function) in functions.iter().enumerate() {
        let base = record_index * TVM_DISPATCH_RECORD_ENTRY_BYTES;
        write_u64(
            module.isa().endianness(),
            &mut record_bytes[base..base + 8],
            function.0,
        );
        write_u32(
            module.isa().endianness(),
            &mut record_bytes[base + TVM_DISPATCH_RECORD_TRANSITION_COUNT_OFFSET
                ..base + TVM_DISPATCH_RECORD_TRANSITION_COUNT_OFFSET + 4],
            u32::try_from(function.3).map_err(|_| {
                "error[cranelift.dispatch]: transition count exceeds u32".to_string()
            })?,
        );
        write_u32(
            module.isa().endianness(),
            &mut record_bytes[base + TVM_DISPATCH_RECORD_SHAPE_OFFSET
                ..base + TVM_DISPATCH_RECORD_SHAPE_OFFSET + 4],
            dispatch_shape(function),
        );
        let function_ref = module.declare_func_in_data(function.2, &mut records_data);
        let relocation_offset = u32::try_from(base + TVM_DISPATCH_RECORD_FUNCTION_POINTER_OFFSET)
            .map_err(|_| {
            "error[cranelift.dispatch]: relocation offset exceeds u32".to_string()
        })?;
        records_data.write_function_addr(relocation_offset, function_ref);
    }
    records_data.define(record_bytes.into_boxed_slice());
    records_data.set_align(8);
    let records_id = module
        .declare_data(DISPATCH_RECORDS_SYMBOL, Linkage::Local, false, false)
        .map_err(|error| format!("error[cranelift.dispatch_records_declare]: {error}"))?;
    module
        .define_data(records_id, &records_data)
        .map_err(|error| format!("error[cranelift.dispatch_records_define]: {error}"))?;
    Ok((index_id, records_id, table_length))
}

fn dispatch_shape(function: &DispatchFunction) -> u32 {
    let has_transitions = u32::from(function.3 > 0);
    let suspending = u32::from(function.4);
    u32::try_from(function.1).expect("validated dispatch arity") << 2
        | has_transitions << 1
        | suspending
}

fn decode_dispatch_shape(shape: u32) -> (usize, bool, bool) {
    ((shape >> 2) as usize, shape & 0b10 != 0, shape & 0b01 != 0)
}

fn write_u64(endianness: Endianness, destination: &mut [u8], value: u64) {
    let bytes = match endianness {
        Endianness::Little => value.to_le_bytes(),
        Endianness::Big => value.to_be_bytes(),
    };
    destination.copy_from_slice(&bytes);
}

fn write_u32(endianness: Endianness, destination: &mut [u8], value: u32) {
    let bytes = match endianness {
        Endianness::Little => value.to_le_bytes(),
        Endianness::Big => value.to_be_bytes(),
    };
    destination.copy_from_slice(&bytes);
}
