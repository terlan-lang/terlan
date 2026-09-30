# Terlan Native Runtime Internals

This directory contains legacy Rust-native adapters still awaiting migration to
their owning standard-library packages. New library implementations belong in
`std/<package>`, not in the compiler or VM.

## Responsibilities

- Own Rust-backed resource behavior such as `std.native.collections.Vector`.
- Keep concrete storage details out of NativeBoundary bridge policy modules.
- Preserve documented, tested, panic-free functions for future verification.
- Keep tests adjacent in separate `*_test.rs` files.

## Public Surface

- `mod.rs`: module exports and safety lints.
- `http.rs`: Rust-backed HTTP request/response/cookie adapter.
- `json` re-export: interim access to the package-owned adapter in
  `std/data/native`; JSON implementation and tests no longer live here.
- `path.rs`: Rust-backed lexical path adapter.
- `postgres.rs`: Rust Postgres pool, query, transaction, and row adapter.
- `vector.rs`: Rust-owned indexed vector resource used through NativeBoundary
  handles.

## Integration Points

- URI, encoding, and Ed25519 copied-value operations are composed in
  `std/native/packages.rs` and dispatched through the generic native boundary.
  Their implementations and tests live under their owning packages.

- `runtime/native_boundary/dispatch.rs` calls native adapters after validating bridge
  operation ids and argument shapes.
- `runtime/native_boundary/resource.rs` stores native adapter resources behind opaque
  handles.
- Compiler lowering emits bridge calls that eventually reach these adapters.

## Testing Notes

- Add one adjacent `*_test.rs` file per implementation module.
- Test adapter behavior directly and through NativeBoundary dispatch when bridge
  behavior is involved.
