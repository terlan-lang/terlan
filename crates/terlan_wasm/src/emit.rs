use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction, Module,
    TypeSection, ValType,
};
use wasmparser::{Validator, WasmFeatures};

use crate::backend_ir::{WasmFunctionBody, WasmModuleIr, WasmResultType};

/// Wasm emission or validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmEmitError {
    EmptyModule,
    EmptyFunctionName,
    EmptyExportName,
    Validation(String),
}

impl std::fmt::Display for WasmEmitError {
    /// Formats a Wasm emission error.
    ///
    /// Inputs: formatter sink.
    /// Output: formatting result.
    /// Transformation: maps each error variant to a stable diagnostic string.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyModule => write!(f, "Wasm module must contain at least one function"),
            Self::EmptyFunctionName => write!(f, "Wasm function name cannot be empty"),
            Self::EmptyExportName => write!(f, "Wasm export name cannot be empty"),
            Self::Validation(message) => write!(f, "Wasm validation failed: {message}"),
        }
    }
}

impl std::error::Error for WasmEmitError {}

/// Emits a valid Wasm module from the minimal backend IR.
///
/// Inputs:
/// - `module_ir`: typed Wasm backend IR.
///
/// Output:
/// - Validated WebAssembly binary bytes.
///
/// Transformation:
/// - Uses `wasm-encoder` for binary construction and validates the result with
///   `wasmparser` before returning it to callers.
pub fn emit_module(module_ir: &WasmModuleIr) -> Result<Vec<u8>, WasmEmitError> {
    validate_ir(module_ir)?;

    let mut module = Module::new();

    let mut types = TypeSection::new();
    for function in &module_ir.functions {
        types.ty().function([], [result_val_type(function.result)]);
    }
    module.section(&types);

    let mut functions = FunctionSection::new();
    for index in 0..module_ir.functions.len() {
        functions.function(index as u32);
    }
    module.section(&functions);

    let mut exports = ExportSection::new();
    for (index, function) in module_ir.functions.iter().enumerate() {
        if let Some(export) = &function.export {
            exports.export(&export.name, ExportKind::Func, index as u32);
        }
    }
    module.section(&exports);

    let mut codes = CodeSection::new();
    for function in &module_ir.functions {
        let mut body = Function::new([]);
        match function.body {
            WasmFunctionBody::I32Const(value) => {
                body.instruction(&Instruction::I32Const(value));
            }
        }
        body.instruction(&Instruction::End);
        codes.function(&body);
    }
    module.section(&codes);

    let bytes = module.finish();
    validate_module(&bytes)?;
    Ok(bytes)
}

/// Validates Wasm module bytes using the Rust parser/validator.
///
/// Inputs:
/// - `bytes`: candidate WebAssembly module bytes.
///
/// Output:
/// - `Ok(())` when the module validates.
/// - `Err(WasmEmitError::Validation)` with the validator diagnostic otherwise.
pub fn validate_module(bytes: &[u8]) -> Result<(), WasmEmitError> {
    Validator::new_with_features(WasmFeatures::default())
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|err| WasmEmitError::Validation(err.to_string()))
}

/// Validates minimal Wasm backend IR before emission.
///
/// Inputs: typed Wasm module IR.
/// Output: success or a stable emission error.
/// Transformation: rejects empty modules and empty function/export names before
/// binary construction starts.
fn validate_ir(module_ir: &WasmModuleIr) -> Result<(), WasmEmitError> {
    if module_ir.functions.is_empty() {
        return Err(WasmEmitError::EmptyModule);
    }
    for function in &module_ir.functions {
        if function.name.trim().is_empty() {
            return Err(WasmEmitError::EmptyFunctionName);
        }
        if let Some(export) = &function.export {
            if export.name.trim().is_empty() {
                return Err(WasmEmitError::EmptyExportName);
            }
        }
    }
    Ok(())
}

/// Converts backend result type to `wasm-encoder` value type.
///
/// Inputs: Terlan Wasm backend result type.
/// Output: encoder value type.
/// Transformation: preserves the scalar result type for section emission.
fn result_val_type(result: WasmResultType) -> ValType {
    match result {
        WasmResultType::I32 => ValType::I32,
    }
}

#[cfg(test)]
#[path = "emit_test.rs"]
mod emit_test;
