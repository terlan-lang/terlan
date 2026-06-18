# Terlan SafeNative Source Internals

This directory owns SafeNative implementation modules. Each module keeps its
tests adjacent in a separate `*_test.rs` file so implementation code remains
readable while test coverage stays close.

## Responsibilities

- Implement native helper modules for JSON, HTTP, URI, path, Base64, request,
  resource, runtime, and worker behavior.
- Keep dispatch inputs and outputs explicit.
- Avoid unsafe code and panic-oriented failure handling.
- Preserve small functions suitable for later verification work.

## Public Surface

- `lib.rs`: crate module exports and safety denies.
- `dispatch.rs`: operation dispatch helpers.
- `resource.rs`, `handle.rs`, `runtime.rs`, and `worker.rs`: runtime state
  helpers.
- Data/protocol helper modules such as `json.rs`, `http.rs`, `uri.rs`, and
  `base64.rs`.

## Core Model

SafeNative code is ordinary Rust helper logic, not generated FFI glue. It
receives typed inputs, validates them, updates explicit state when needed, and
returns structured results.

The main flow is:

1. Decode or receive a typed operation request.
2. Validate input terms, handles, and resource state.
3. Execute the small native helper.
4. Return a typed value or structured error.

Important invariants:

- No unsafe Rust.
- No unchecked unwrap, expect, panic, todo, or unimplemented paths.
- Tests remain outside implementation files.

## Integration Points

- Standard-library native operation metadata.
- CLI/runtime dispatch code.
- Future proof tools that inspect or mirror these small Rust functions.

## Edge Cases

- Resource lookup failures must not mutate unrelated state.
- Invalid input terms must return errors, not panic.
- Pure operations should remain independent from runtime worker state.

## Types And Interfaces

`dispatch`
: Maps operation identifiers to native helper behavior.

`runtime`
: Owns runtime state transitions for resources and workers.

`term`
: Defines boundary values accepted by native helpers.

## Testing Notes

- Add one adjacent `*_test.rs` file per implementation module.
- Keep tests focused on value transformation, invalid input, and state
  transition behavior.
- Run `cargo test -p terlan_safenative` for this crate.
