use cranelift_codegen::ir::{self, AbiParam, InstBuilder, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::{function::declare_managed_tail_roots, RUNTIME_ARGUMENT_COUNT};

#[test]
fn managed_tail_parameter_live_across_cranelift_safepoint_emits_precise_stack_map() {
    let mut signature = Signature::new(CallConv::SystemV);
    for _ in 0..RUNTIME_ARGUMENT_COUNT + 1 {
        signature.params.push(AbiParam::new(ir::types::I64));
    }
    signature.returns.push(AbiParam::new(ir::types::I64));
    let mut function = ir::Function::with_name_signature(
        ir::UserFuncName::testcase("terlan_managed_safepoint"),
        signature,
    );
    let mut frontend_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut frontend_context);
    let imported_name = builder
        .func
        .declare_imported_user_function(ir::UserExternalName {
            namespace: 0,
            index: 0,
        });
    let imported_signature = builder
        .func
        .import_signature(Signature::new(CallConv::SystemV));
    let safepoint = builder.import_function(ir::ExtFuncData {
        name: ir::ExternalName::user(imported_name),
        signature: imported_signature,
        colocated: true,
        patchable: false,
    });
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    let params = builder.block_params(entry).to_vec();
    let managed_reference = params[RUNTIME_ARGUMENT_COUNT];
    declare_managed_tail_roots(&mut builder, &params, &[true])
        .expect("declare typed managed tail root");
    builder.ins().call(safepoint, &[]);
    builder.ins().return_(&[managed_reference]);
    builder.seal_all_blocks();
    builder.finalize();

    let emitted = function.display().to_string();
    assert!(emitted.contains("stack_store"), "{emitted}");
    assert!(emitted.contains("stack_map=[i64 @"), "{emitted}");
    assert!(emitted.contains("stack_load.i64"), "{emitted}");
}
