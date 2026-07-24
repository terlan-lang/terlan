//! Exported direct-AOT dispatcher emission.

use cranelift_codegen::ir::{
    condcodes::IntCC, types, AbiParam, Function, InstBuilder, MemFlagsData, Signature, UserFuncName,
};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use super::super::{status, DISPATCH_SYMBOL};
use super::setup::declare_image_func_in_func;
use super::transition::transition_flags;
use super::RUNTIME_ARGUMENT_COUNT;

/// Returns the frozen exported image-dispatch signature.
pub(super) fn dispatch_signature(module: &ObjectModule) -> Signature {
    let pointer = module.target_config().pointer_type();
    Signature {
        params: vec![
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

/// Defines the stable exported dispatcher over all generated native entries.
pub(super) fn define_dispatch(
    module: &mut ObjectModule,
    functions: &[(u64, usize, FuncId, usize, bool)],
) -> Result<(), String> {
    let signature = dispatch_signature(module);
    let dispatch_id = module
        .declare_function(DISPATCH_SYMBOL, Linkage::Export, &signature)
        .map_err(|error| format!("error[cranelift.dispatch_declare]: {error}"))?;
    let mut context = Context::new();
    context.func =
        Function::with_name_signature(UserFuncName::user(0, dispatch_id.as_u32()), signature);
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
        let export_id = params[3];
        let args_pointer = params[4];
        let supplied_arity = params[5];
        let result_pointer = params[6];
        let transition_pointer = params[7];
        let transition_capacity = params[8];
        let transition_len_pointer = params[9];
        let zero = builder.ins().iconst(types::I64, 0);
        builder
            .ins()
            .store(MemFlagsData::new(), zero, transition_len_pointer, 0);

        for (function_id_value, function_arity, function_id, transition_count, suspending) in
            functions
        {
            let matched = builder.create_block();
            let unmatched = builder.create_block();
            let expected_id = builder.ins().iconst(types::I64, *function_id_value as i64);
            let is_match = builder.ins().icmp(IntCC::Equal, export_id, expected_id);
            builder.ins().brif(is_match, matched, &[], unmatched, &[]);
            builder.switch_to_block(matched);

            let expected_arity = builder.ins().iconst(types::I64, *function_arity as i64);
            let arity_matches = builder
                .ins()
                .icmp(IntCC::Equal, supplied_arity, expected_arity);
            let invoke = builder.create_block();
            let arity_error = builder.create_block();
            builder
                .ins()
                .brif(arity_matches, invoke, &[], arity_error, &[]);
            builder.switch_to_block(arity_error);
            let arity = builder.ins().iconst(types::I32, i64::from(status::ARITY));
            builder.ins().return_(&[arity]);

            builder.switch_to_block(invoke);
            let required_capacity = builder.ins().iconst(types::I64, *transition_count as i64);
            let capacity_sufficient = builder.ins().icmp(
                IntCC::UnsignedGreaterThanOrEqual,
                transition_capacity,
                required_capacity,
            );
            let call_block = builder.create_block();
            let capacity_error = builder.create_block();
            builder
                .ins()
                .brif(capacity_sufficient, call_block, &[], capacity_error, &[]);
            builder.switch_to_block(capacity_error);
            let capacity = builder
                .ins()
                .iconst(types::I32, i64::from(status::TRANSITION_CAPACITY));
            builder.ins().return_(&[capacity]);

            builder.switch_to_block(call_block);
            let mut args =
                Vec::with_capacity(function_arity.saturating_add(RUNTIME_ARGUMENT_COUNT));
            args.extend([runtime_context, managed_allocator, closure_resolver]);
            for argument_index in 0..*function_arity {
                let offset = i32::try_from(argument_index.saturating_mul(8)).map_err(|_| {
                    "error[cranelift.dispatch]: argument offset exceeds i32".to_string()
                })?;
                args.push(builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    args_pointer,
                    offset,
                ));
            }
            if *transition_count > 0 {
                args.push(transition_pointer);
            }
            if *suspending {
                args.push(transition_len_pointer);
            }
            let function_ref = declare_image_func_in_func(module, *function_id, builder.func);
            let call = builder.ins().call(function_ref, &args);
            let results = builder.inst_results(call).to_vec();
            let call_status = results[0];
            let value = results[1];
            let succeeded =
                builder
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

            builder.switch_to_block(unmatched);
        }
        let unknown = builder
            .ins()
            .iconst(types::I32, i64::from(status::UNKNOWN_EXPORT));
        builder.ins().return_(&[unknown]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(dispatch_id, &mut context)
        .map_err(|error| format!("error[cranelift.dispatch_define]: {error}"))
}
