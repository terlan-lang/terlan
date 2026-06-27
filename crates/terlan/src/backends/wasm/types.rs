/// WebAssembly ABI scalar type admitted by the initial Terlan Wasm backend.
///
/// Inputs:
/// - Produced by future Terlan type lowering.
///
/// Output:
/// - Stable ABI type used by Wasm function signatures and value checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WasmAbiType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmAbiType {
    /// Returns the canonical Wasm ABI spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// WebAssembly ABI value admitted by the initial Terlan Wasm backend.
///
/// Inputs:
/// - Produced by constants, test harnesses, or future host-boundary adapters.
///
/// Output:
/// - Typed scalar value that can be checked against an ABI signature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl WasmValue {
    /// Returns the ABI type of this value.
    pub const fn abi_type(self) -> WasmAbiType {
        match self {
            Self::I32(_) => WasmAbiType::I32,
            Self::I64(_) => WasmAbiType::I64,
            Self::F32(_) => WasmAbiType::F32,
            Self::F64(_) => WasmAbiType::F64,
        }
    }
}
