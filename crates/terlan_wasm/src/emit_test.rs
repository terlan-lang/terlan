use super::*;
use crate::backend_ir::{WasmFunction, WasmModuleIr};

#[test]
fn emit_module_validates_exported_i32_const_function() {
    let module_ir = WasmModuleIr::new(vec![WasmFunction::exported_i32_const("answer", 42)]);

    let bytes = emit_module(&module_ir).expect("module should emit");

    validate_module(&bytes).expect("module should validate");
}

#[test]
fn validate_module_rejects_invalid_bytes() {
    let err = validate_module(b"not wasm").expect_err("invalid bytes should fail");

    assert!(matches!(err, WasmEmitError::Validation(_)));
}

#[test]
fn emit_module_rejects_empty_module() {
    let err =
        emit_module(&WasmModuleIr::new(Vec::new())).expect_err("empty module should be rejected");

    assert_eq!(err, WasmEmitError::EmptyModule);
}
