# Wasm Source Internals

This directory owns Rust-side WebAssembly backend implementation for the
shipped `terlan` crate. The implementation is centered on separate files for
ABI modeling, backend IR, CoreIR lowering, emission, and scalar value types.
Its most important boundary is keeping tests adjacent while avoiding inline
test bodies in implementation files.

## Responsibilities

- Keep Wasm ABI, IR, CoreIR lowering, emission, and value models separated by
  file.
- Use maintained Rust crates for binary emission and validation.
- Provide small public APIs re-exported by the crate root.
- Keep tests in `*_test.rs` files beside the implementation they cover.

## Public Surface

- `abi`: ABI signature and result validation.
- `backend_ir`: minimal typed backend IR.
- `lower`: first checked CoreIR-to-Wasm backend IR lowering slice.
- `emit`: Wasm byte emission and validation.
- `types`: scalar Wasm ABI values and type labels.

## Core Model

The source layout mirrors the backend flow: type/value definitions feed ABI and
IR modeling, checked CoreIR lowers into backend IR, then the emitter produces
validated bytes from IR.

The main flow is:

1. Represent backend intent with `backend_ir`.
2. Represent host-boundary scalar values with `types` and `abi`.
3. Lower checked CoreIR into backend IR with `lower`.
4. Emit and validate binaries in `emit`.

Important invariants:

- Implementation files stay free of inline test bodies.
- Emitted bytes are validated before returning to callers.
- Public types remain small until CoreIR lowering requires expansion.

## Integration Points

- Crate root `lib.rs`: public re-export surface.
- `wasm-encoder`: byte construction in `emit`.
- `wasmparser`: byte validation in `emit`.

## Edge Cases

- Empty backend modules fail before emission.
- Empty function/export names fail before emission.
- Unsupported ABI shapes produce explicit errors.
- Unsupported CoreIR forms fail during lowering before byte emission.
- The first lowering slice maps Terlan `Int` parameters and return values to
  Wasm `i32`; other ABI types stay explicit unsupported cases until
  `std.wasm.Abi` inference lands.
- Terlan `Bool` export results are represented as Wasm `i32` for the first
  integer comparison slice.

## Types And Interfaces

`WasmAbiType`
: Scalar ABI type label.

`WasmEmitError`
: Stable emission and validation error.

`WasmLowerError`
: Stable CoreIR lowering error.

## Testing Notes

- `abi_test.rs` covers ABI result matching.
- `lower_test.rs` covers checked CoreIR-to-Wasm backend IR lowering.
- `emit_test.rs` covers binary emission and validation.
- New modules should add sibling `*_test.rs` files rather than inline tests.
