//! Platform linker entry emission for direct-AOT native images.

use cranelift_codegen::ir::{types, AbiParam, Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;

use super::super::IMAGE_ENTRY_SYMBOL;

/// Defines the native-image entry marker used by platform linkers.
///
/// On Windows this is also a valid successful DLL entry: the platform may pass
/// its three loader arguments, which the supported 64-bit calling conventions
/// permit this no-argument callee to ignore.
pub(super) fn define_image_entry(module: &mut ObjectModule) -> Result<(), String> {
    let signature = Signature {
        params: Vec::new(),
        returns: vec![AbiParam::new(types::I32)],
        call_conv: module.target_config().default_call_conv,
    };
    let entry_id = module
        .declare_function(IMAGE_ENTRY_SYMBOL, Linkage::Export, &signature)
        .map_err(|error| format!("error[cranelift.image_entry_declare]: {error}"))?;
    let mut context = Context::new();
    context.func =
        Function::with_name_signature(UserFuncName::user(0, entry_id.as_u32()), signature);
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        let success = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[success]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(entry_id, &mut context)
        .map_err(|error| format!("error[cranelift.image_entry_define]: {error}"))
}
