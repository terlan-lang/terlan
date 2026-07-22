# Std WASM Internals

This directory owns WebAssembly ABI-facing standard-library types.

## Responsibilities

- Define portable WASM ABI marker types.
- Keep WASM target declarations expressible from Terlan types.
- Avoid duplicating target metadata that the compiler can infer.

## Public Surface

- `Abi.terl`: `I32`, `I64`, `F32`, and `F64` scalar ABI markers.

## Executable Contract

The declaration shape is ordinary Terlan syntax. Importing an ABI marker is
enough for target inference; no export annotation is required:

```terlan
import std.wasm.Abi.{F32, F64, I32, I64}.

pub identity_i32(value: I32): I32 -> value.
pub identity_i64(value: I64): I64 -> value.
pub identity_f32(value: F32): F32 -> value.
pub identity_f64(value: F64): F64 -> value.
```

`terlc test --target wasm`, `terlc build`, and `terlc run` consume the same
signature. The compiler labels the browser/component/WASI families as
`reserved`; their use never falls back to JavaScript. Aggregate values,
strings, binaries, references, and multiple results are `unsupported` until
their ABI contracts are implemented.

## Invariants

- WASM ABI types are target-facing contracts, not general numeric aliases.
- Compiler inference should use these types before requiring explicit target
  annotations.
- Non-WASM targets must reject WASM-only surface through target validation.
- ABI manifests carry a namespace contract checksum and an export-signature
  checksum. Runtime execution rejects stale artifacts before loading bytes.

## Testing Notes

- WASM lowering and ABI validation gates should add focused fixtures as the
  target matures.
