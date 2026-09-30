# VM WebSocket Internals

This directory contains WebSocket-specific memory accounting and legacy VM
session integration. Live wire protocol state and upgrade response construction
belong to `std/http/native/src/websocket.rs`, backed by maintained tungstenite.

## Responsibilities

- Charge queued frames and payloads to the owning VM connection process.
- Reject queue growth before memory limits are exceeded.
- Release all charges on delivery, cancellation, close, and owner exit.
- Retain closure-free callback identities in immutable endpoint plans while
  the serve adapter owns callback invocation state.

## Integration Points

- `std.http` native codec: owns live protocol parsing, framing, and handshake
  response metadata without scheduling or waiting for I/O.
- `runtime::vm::websocket`: retains connection lifecycle and bounded queues;
  this HTTP-specific runtime surface still requires ownership migration.
- `runtime::vm::native_callable`: owns the shared static generated-call
  identity used by HTTP and WebSocket adapters.
- `runtime::vm::memory`: owns aggregate process limits and cleanup.

## Testing Notes

- `memory_test.rs` covers charging, rejection, and terminal cleanup.
- Legacy in-memory VM protocol helpers still use tungstenite as a test-only
  dependency. Live serving uses the package codec, not those helpers.
- Add fragmented-frame and cancellation-race coverage when queue semantics
  change.
