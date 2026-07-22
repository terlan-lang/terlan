# VM WebSocket Internals

This directory owns WebSocket-specific memory accounting while the parent
module owns frame parsing, connection state, queues, and close semantics.

## Responsibilities

- Charge queued frames and payloads to the owning VM connection process.
- Reject queue growth before memory limits are exceeded.
- Release all charges on delivery, cancellation, close, and owner exit.
- Retain closure-free callback identities in immutable endpoint plans while
  the serve adapter owns callback invocation state.

## Integration Points

- `runtime::vm::websocket`: owns protocol and connection lifecycle.
- `runtime::vm::native_callable`: owns the shared static generated-call
  identity used by HTTP and WebSocket adapters.
- `runtime::vm::memory`: owns aggregate process limits and cleanup.

## Testing Notes

- `memory_test.rs` covers charging, rejection, and terminal cleanup.
- Add fragmented-frame and cancellation-race coverage when queue semantics
  change.
