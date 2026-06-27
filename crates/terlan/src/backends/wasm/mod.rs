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

#![allow(dead_code, unused_imports)]

pub(crate) mod abi;
pub(crate) mod backend_ir;
pub(crate) mod emit;
pub(crate) mod types;

pub(crate) use abi::{validate_export_result_value, WasmAbiError, WasmFunctionAbi};
pub(crate) use backend_ir::{WasmExport, WasmFunction, WasmModuleIr, WasmResultType};
pub(crate) use emit::{emit_module, validate_module, WasmEmitError};
pub(crate) use types::{WasmAbiType, WasmValue};
