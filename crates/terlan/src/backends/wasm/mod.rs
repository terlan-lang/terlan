//! Rust-owned WebAssembly backend tooling for Terlan.
//!
//! Inputs:
//! - Future CoreIR-to-Wasm lowering output and host ABI declarations.
//!
//! Outputs:
//! - Validated WebAssembly bytes plus stable ABI metadata.
//!
//! Transformation:
//! - Defines a typed backend IR boundary, emits Wasm through `wasm-encoder`,
//!   and validates bytes through `wasmparser`.
pub(crate) mod abi;
pub(crate) mod backend_ir;
pub(crate) mod contract;
pub(crate) mod emit;
pub(crate) mod lower;
pub(crate) mod types;

pub(crate) use backend_ir::{WasmFunction, WasmModuleIr, WasmResultType};
pub(crate) use contract::{
    wasm_abi_contract_checksum, wasm_abi_signature_checksum, wasm_checksum, WasmAbiSignature,
};
pub(crate) use emit::{emit_module, validate_module, WasmEmitError};
pub(crate) use lower::{lower_core_module, WasmLowerError};
