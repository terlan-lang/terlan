/// Minimal Wasm backend module IR.
///
/// Inputs:
/// - Produced by future Terlan CoreIR-to-Wasm lowering.
///
/// Output:
/// - Typed module shape accepted by the Wasm emitter.
///
/// Transformation:
/// - Keeps exported functions explicit so CLI/package metadata can validate
///   the Wasm boundary before binary emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmModuleIr {
    pub functions: Vec<WasmFunction>,
}

impl WasmModuleIr {
    /// Builds a module IR from exported functions.
    pub fn new(functions: Vec<WasmFunction>) -> Self {
        Self { functions }
    }
}

/// Minimal Wasm function IR.
///
/// Inputs:
/// - Produced by future backend lowering.
///
/// Output:
/// - Function signature, constant body placeholder, and optional export name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFunction {
    pub name: String,
    pub result: WasmResultType,
    pub body: WasmFunctionBody,
    pub export: Option<WasmExport>,
}

impl WasmFunction {
    /// Creates an exported i32 constant function.
    pub fn exported_i32_const(name: impl Into<String>, value: i32) -> Self {
        let name = name.into();
        Self {
            export: Some(WasmExport { name: name.clone() }),
            name,
            result: WasmResultType::I32,
            body: WasmFunctionBody::I32Const(value),
        }
    }
}

/// Wasm function export metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmExport {
    pub name: String,
}

/// Minimal supported Wasm result type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmResultType {
    I32,
}

/// Minimal supported Wasm function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmFunctionBody {
    I32Const(i32),
}
