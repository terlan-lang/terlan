# Terlan Wasm Crate Internals

This crate owns Terlan's Rust WebAssembly backend boundary. The implementation
is centered on a small typed backend IR, ABI checks, binary emission through
`wasm-encoder`, and validation through `wasmparser`. Its most important
boundary is that it provides backend infrastructure without owning full Terlan
CoreIR lowering yet.

## Responsibilities

- Define the initial Wasm backend IR.
- Define scalar ABI values and ABI validation errors.
- Emit WebAssembly binaries through maintained Rust tooling.
- Validate emitted or supplied bytes before they cross backend boundaries.

## Public Surface

- `WasmModuleIr`: minimal module-level backend IR.
- `emit_module`: emits validated Wasm bytes.
- `validate_module`: validates Wasm bytes through `wasmparser`.
- `validate_export_result_value`: checks scalar export smoke results against an
  ABI signature.

## Core Model

The crate currently supports exported constant functions as a release-safe
anchor for future CoreIR-to-Wasm lowering. The ABI model supports scalar Wasm
types and reserves multi-value results until the caller/runtime contract is
expanded.

The main flow is:

1. Build typed Wasm backend IR.
2. Validate the IR for empty names and empty modules.
3. Emit bytes with `wasm-encoder` and validate them with `wasmparser`.

Important invariants:

- Emission must use maintained Rust crates, not hand-written Wasm bytes.
- Invalid IR must fail before bytes are returned.
- ABI validation must produce stable errors for host-boundary tests.

## Integration Points

- `wasm-encoder`: binary module construction.
- `wasmparser`: post-emission validation.
- Future backend lowering: producer of `WasmModuleIr`.

## Edge Cases

- Empty modules and empty names are rejected before emission.
- Invalid byte sequences are reported as validation errors.
- Multi-value ABI results are intentionally reserved.

## Types And Interfaces

`WasmModuleIr`
: Minimal module shape accepted by the emitter.

`WasmFunctionAbi`
: Export ABI signature consumed by scalar result validation.

## Testing Notes

- `src/emit_test.rs` covers byte emission and validation failures.
- `src/abi_test.rs` covers scalar ABI acceptance and rejection.
- Add tests beside the module that owns any new backend feature.
