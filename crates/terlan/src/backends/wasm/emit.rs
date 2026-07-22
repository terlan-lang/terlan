use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, Ieee32, Ieee64, Instruction,
    Module, TypeSection, ValType,
};
use wasmparser::{Validator, WasmFeatures};

use super::backend_ir::{WasmFunctionBody, WasmInstruction, WasmModuleIr, WasmResultType};

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
        let params = function
            .params
            .iter()
            .map(|param| result_val_type(param.ty))
            .collect::<Vec<_>>();
        types
            .ty()
            .function(params, [result_val_type(function.result)]);
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
        let WasmFunctionBody::Instructions(instructions) = &function.body;
        for instruction in instructions {
            emit_instruction(&mut body, *instruction);
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
        WasmResultType::I64 => ValType::I64,
        WasmResultType::F32 => ValType::F32,
        WasmResultType::F64 => ValType::F64,
    }
}

/// Emits one typed backend instruction into a Wasm function body.
///
/// Inputs:
/// - `body`: mutable encoder function body.
/// - `instruction`: typed Wasm backend instruction.
///
/// Output:
/// - Appends the corresponding `wasm-encoder` instruction.
///
/// Transformation:
/// - Maps the backend IR one-to-one into maintained encoder calls.
fn emit_instruction(body: &mut Function, instruction: WasmInstruction) {
    match instruction {
        WasmInstruction::I32Const(value) => {
            body.instruction(&Instruction::I32Const(value));
        }
        WasmInstruction::I64Const(value) => {
            body.instruction(&Instruction::I64Const(value));
        }
        WasmInstruction::F32ConstBits(value) => {
            body.instruction(&Instruction::F32Const(Ieee32::new(value)));
        }
        WasmInstruction::F64ConstBits(value) => {
            body.instruction(&Instruction::F64Const(Ieee64::new(value)));
        }
        WasmInstruction::LocalGet(index) => {
            body.instruction(&Instruction::LocalGet(index));
        }
        WasmInstruction::I32Add => {
            body.instruction(&Instruction::I32Add);
        }
        WasmInstruction::I32Sub => {
            body.instruction(&Instruction::I32Sub);
        }
        WasmInstruction::I32Mul => {
            body.instruction(&Instruction::I32Mul);
        }
        WasmInstruction::I32Eq => {
            body.instruction(&Instruction::I32Eq);
        }
        WasmInstruction::I32Ne => {
            body.instruction(&Instruction::I32Ne);
        }
        WasmInstruction::I32LtS => {
            body.instruction(&Instruction::I32LtS);
        }
        WasmInstruction::I32LeS => {
            body.instruction(&Instruction::I32LeS);
        }
        WasmInstruction::I32GtS => {
            body.instruction(&Instruction::I32GtS);
        }
        WasmInstruction::I32GeS => {
            body.instruction(&Instruction::I32GeS);
        }
    };
}

#[cfg(test)]
#[path = "emit_test.rs"]
mod emit_test;
