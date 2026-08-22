//! Cross-target regression coverage for image-local function relocations.

use cranelift_codegen::ir::{Function, InstBuilder, UserFuncName};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};

use super::super::NativeCodegenPolicy;
use super::setup::{declare_image_func_in_func, object_module_for_isa};

#[test]
fn image_local_function_calls_emit_on_windows_aarch64_coff() {
    let isa = cranelift_codegen::isa::lookup_by_name("aarch64-pc-windows-msvc")
        .expect("Windows AArch64 ISA");
    let mut module = object_module_for_isa(
        "windows_aarch64_image_call",
        NativeCodegenPolicy::Development,
        isa,
    )
    .expect("Windows AArch64 object module");
    let signature = module.make_signature();
    let callee = module
        .declare_function("image_callee", Linkage::Local, &signature)
        .expect("declare image callee");
    let caller = module
        .declare_function("image_caller", Linkage::Export, &signature)
        .expect("declare image caller");

    define_return_only_function(&mut module, callee, signature.clone());

    let mut context = Context::new();
    context.func = Function::with_name_signature(UserFuncName::user(0, caller.as_u32()), signature);
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        let callee_ref = declare_image_func_in_func(&mut module, callee, builder.func);
        builder.ins().call(callee_ref, &[]);
        builder.ins().return_(&[]);
        builder.finalize();
    }
    module
        .define_function(caller, &mut context)
        .expect("define image caller");

    let object = module.finish().emit().expect("emit Windows AArch64 COFF");
    assert!(
        !object.is_empty(),
        "Windows AArch64 object must not be empty"
    );
}

fn define_return_only_function(
    module: &mut cranelift_object::ObjectModule,
    function_id: cranelift_module::FuncId,
    signature: cranelift_codegen::ir::Signature,
) {
    let mut context = Context::new();
    context.func =
        Function::with_name_signature(UserFuncName::user(0, function_id.as_u32()), signature);
    let mut frontend_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut frontend_context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.ins().return_(&[]);
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .expect("define image callee");
}
