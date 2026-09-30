# NativeBoundary HTTP Internals

This directory retains compiler request-projection metadata during the standard
library ownership migration. Request/response host adapters, cookie parsing,
serialization, and jar storage live in `std/http/native`; `../http.rs` is a
compatibility reexport of that package's types and helpers.
Maintained Rust libraries own protocol codecs, while Terlan modules own library
policy. The remaining compiler/runtime HTTP-specific representation and lowering
are migration work, not the intended final ownership boundary.

## Responsibilities

- Describe compiler request-field projections without owning host HTTP adapters.
- Preserve typed boundaries for Terlan web handlers.
- Keep protocol handling delegated to maintained Rust crates.

## Public Surface

- `../http.rs`: compatibility facade and compiler request-projection metadata.
- `std/http/native`: package-owned host adapters, cookie codec, and value bindings.

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

- NativeBoundary dispatch: exposes HTTP runtime operations.
- CLI serve/build commands: consume HTTP runtime behavior.

## Testing Notes

- Add integration tests around request/response conversion and routing.
