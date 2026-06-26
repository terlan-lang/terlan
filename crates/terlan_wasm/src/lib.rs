//! Rust-owned WebAssembly backend tooling for Terlan.
//!
//! The crate starts as a small executable roadmap anchor: it defines a typed
//! backend IR boundary, emits Wasm through `wasm-encoder`, and validates bytes
//! through `wasmparser`. Terlan compiler lowering lands behind this boundary in
//! later gates.

pub mod abi;
pub mod backend_ir;
pub mod emit;
pub mod types;

pub use abi::{validate_export_result_value, WasmAbiError, WasmFunctionAbi};
pub use backend_ir::{WasmExport, WasmFunction, WasmModuleIr, WasmResultType};
pub use emit::{emit_module, validate_module, WasmEmitError};
pub use types::{WasmAbiType, WasmValue};
