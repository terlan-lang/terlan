# Terlan NativeBoundary Source Internals

This directory owns NativeBoundary implementation modules. Each module keeps its
tests adjacent in a separate `*_test.rs` file so implementation code remains
readable while test coverage stays close.

## Responsibilities

- Implement bridge helper modules for request, resource, runtime, metadata, and
  worker behavior.
- Model async request cancellation and timeout as terminal request states that
  release credits and reject late replies.
- Carry one cloneable request-scoped cancellation token from capability
  transport admission through executor and resource-dispatch checkpoints.
- Keep concrete native-backed resources, such as Vector, under
  `runtime/native` so NativeBoundary stays focused on bridge and safety contracts.
- Keep dispatch inputs and outputs explicit.
- Avoid unsafe code and panic-oriented failure handling.
- Preserve small functions suitable for later verification work.

## Public Surface

- `lib.rs`: crate module exports and safety denies.
- `dispatch.rs`: operation dispatch helpers.
- `adapter_abi.rs`: the versioned public C/C++ adapter contract, target calling
  conventions, bounded transfer policy, and native cache identity.
- `capability_wire.rs`: one versioned, bounded request/response schema shared
  by the VM and external capability-worker process.
- `cancellation.rs`: monotonic cross-thread cooperative cancellation signal
  consumed by long-running capability adapters.
- `capability_sandbox.rs`: fixed profile identity, paths, environment, and
  resource limits shared by the launcher and in-worker attestation, plus the
  fail-closed host capability matrix.
- `metadata.rs`: static worker ownership and bridge-selection contracts.
- `resource.rs`, `handle.rs`, `runtime.rs`, and `worker.rs`: runtime state
  helpers.
- Data/protocol bridge modules that mediate native adapters through handles.
- `runtime/native`: concrete Rust-native adapters used through NativeBoundary
  bridge dispatch.

## Core Model

NativeBoundary code is ordinary Rust helper logic, not generated FFI glue. It
receives typed inputs, validates them, updates explicit state when needed, and
returns structured results.

The public adapter ABI is separate from the private `.tvm` execution ABI. It
uses opaque handles, explicit execution context and ownership, bounded frames,
status values, scoped resources, forbidden callback reentrancy, and single-shot
completion. It never exposes actor heaps, managed runtime references,
continuation layouts, backend signatures, stack addresses, or shard identity.

The main flow is:

1. Decode or receive a typed operation request.
2. Validate input terms, handles, and resource state.
3. Reject cancellation before adapter entry and expose the request token to
   long-running capability executors for polling during work.
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
- Cancelled and timed-out requests must not be completed by late native replies.
- Only manifest exports marked cooperative may acknowledge cancellation, and
  the cancelling process must match the request owner.
- Capability frames contain owned values and opaque handle identities only;
  application heaps, continuations, and native pointers never cross the wire.
- Native AOT support on a host does not imply external capability-worker
  support. Version 0.0.7 admits the attested Linux profile only; other hosts
  reject external capability work before allocating or spawning anything.

## Types And Interfaces

`dispatch`
: Maps operation identifiers to native helper behavior.

`runtime`
: Owns runtime state transitions for resources and workers.

`metadata`
: Describes worker-level adapter ownership before transport-specific code is
  connected.

`term`
: Defines boundary values accepted by native helpers.

## Testing Notes

- Add one adjacent `*_test.rs` file per implementation module.
- Keep tests focused on value transformation, invalid input, and state
  transition behavior.
- Run `cargo test -p terlan terlan_native_boundary::` for this feature area.
