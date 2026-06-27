# Terlan Native Runtime Internals

This directory owns concrete Rust-native adapter implementations used by
standard-library modules.

## Responsibilities

- Own Rust-backed resource behavior such as `std.native.collections.Vector`.
- Keep concrete storage details out of SafeNative bridge policy modules.
- Preserve documented, tested, panic-free functions for future verification.
- Keep tests adjacent in separate `*_test.rs` files.

## Public Surface

- `mod.rs`: module exports and safety lints.
- `base64.rs`: Rust-backed Base64 encoding and decoding adapter.
- `http.rs`: Rust-backed HTTP request/response/cookie adapter.
- `json.rs`: Rust-backed JSON value, parser, and encoder adapter.
- `path.rs`: Rust-backed lexical path adapter.
- `postgres.rs`: Rust/Tokio Postgres pool, query, transaction, and row adapter.
- `uri.rs`: Rust-backed URI parser and renderer adapter.
- `vector.rs`: Rust-owned indexed vector resource used through SafeNative
  handles.

## Integration Points

- `runtime/safenative/dispatch.rs` calls native adapters after validating bridge
  operation ids and argument shapes.
- `runtime/safenative/resource.rs` stores native adapter resources behind opaque
  handles.
- Compiler lowering emits bridge calls that eventually reach these adapters.

## Testing Notes

- Add one adjacent `*_test.rs` file per implementation module.
- Test adapter behavior directly and through SafeNative dispatch when bridge
  behavior is involved.
