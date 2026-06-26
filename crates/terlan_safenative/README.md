# Terlan SafeNative Crate Internals

This crate owns proof-trackable native helper logic for Terlan runtime
features. It deliberately avoids unsafe code, FFI, NIF entry points, async
runtimes, and generated adapters so the core state transitions stay small and
testable.

## Responsibilities

- Implement pure SafeNative helper operations.
- Model runtime resources, handles, credits, terms, and dispatch results.
- Provide Rust-backed implementations for selected std operations.
- Record worker metadata for supervised native adapters such as Postgres.
- Keep safety-critical native logic separate from generated adapter stubs.

## Public Surface

- Resource and handle helpers used by native operation dispatch.
- JSON, URI, path, Base64, request, and HTTP helper modules.
- Runtime and worker state-transition helpers.
- Worker metadata contracts for adapter ownership, runtime selection, resource
  policy, and operation ids.

## Core Model

SafeNative helpers operate on explicit values and resource state. They are
designed to be unit-tested in Rust and small enough to mirror in future proof
artifacts.

The main flow is:

1. A compiler/runtime boundary selects a native operation.
2. SafeNative validates terms, resource handles, or request state.
3. The helper returns a typed result without panics or unsafe behavior.

Important invariants:

- Unsafe Rust is forbidden.
- Panics, unwraps, todos, and unimplemented branches are denied.
- Long-lived native resources are explicit and handle-based.
- Worker metadata must describe the bridge contract before live sockets or
  backend-specific adapter code are connected.

## Integration Points

- `std/RUST_BACKED_MANIFEST.tsv`: declares std operation ownership.
- `std/summaries/*.safe_native.json`: records generated native metadata.
- `terlan_cli`: invokes SafeNative checks and runtime helpers.
- Future proof tooling: may mirror this crate's small transition functions.

## Edge Cases

- Invalid handles must return structured errors.
- Resource ownership and cleanup must be explicit.
- Pure helpers should not require BEAM process wrappers.

## Types And Interfaces

`Handle`
: Typed identifier for runtime-owned native resources.

`Resource`
: Runtime-managed native resource state.

`Term`
: Boundary value representation for native dispatch.

`WorkerSpec`
: Static metadata describing one supervised SafeNative worker adapter.

## Testing Notes

- Each implementation module has an adjacent `*_test.rs` file.
- `cargo test -p terlan_safenative` should cover pure helper behavior.
- Safety rules are enforced by crate-level denies in `src/lib.rs`.
