# SafeNative HTTP Internals

This directory owns native HTTP runtime support. The implementation is centered
on Rust HTTP crates rather than custom protocol code. Its most important
boundary is that Terlan handlers use typed runtime APIs while HTTP protocol
details stay in Rust libraries.

## Responsibilities

- Provide native HTTP request and response runtime helpers.
- Preserve typed boundaries for Terlan web handlers.
- Keep protocol handling delegated to maintained Rust crates.

## Public Surface

- `mod.rs`: HTTP runtime types and helper entry points.

## Core Model

The HTTP runtime adapts Terlan handler calls to native Rust HTTP execution.

The main flow is:

1. Receive a runtime request from the server stack.
2. Convert it into Terlan-visible request data.
3. Convert handler output back into an HTTP response.

Important invariants:

- Terlan code must not depend on custom HTTP parsing.
- Response conversion must be explicit and typed.
- Runtime errors must remain observable by the CLI/server.

## Integration Points

- SafeNative dispatch: exposes HTTP runtime operations.
- CLI serve/build commands: consume HTTP runtime behavior.

## Testing Notes

- Add integration tests around request/response conversion and routing.
