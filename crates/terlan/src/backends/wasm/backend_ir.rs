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
/// - Function signature, i32 instruction body, and optional export name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFunction {
    pub name: String,
    pub params: Vec<WasmParam>,
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
            params: Vec::new(),
            result: WasmResultType::I32,
            body: WasmFunctionBody::Instructions(vec![WasmInstruction::I32Const(value)]),
        }
    }
}

/// Wasm function export metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmExport {
    pub name: String,
}

/// Wasm function parameter metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmParam {
    pub name: String,
    pub ty: WasmResultType,
}

/// Minimal supported Wasm result type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmResultType {
    I32,
    I64,
    F32,
    F64,
}

/// Minimal supported Wasm function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmFunctionBody {
    Instructions(Vec<WasmInstruction>),
}

/// Minimal supported Wasm instruction subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmInstruction {
    I32Const(i32),
    I64Const(i64),
    F32ConstBits(u32),
    F64ConstBits(u64),
    LocalGet(u32),
    I32Add,
    I32Sub,
    I32Mul,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
}
