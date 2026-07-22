# Runtime Internals

This directory owns Rust runtime services used by Terlan artifacts.

## Responsibilities

- Keep concrete Rust-native implementations separate from bridge policy.
- Own NativeBoundary request, resource, dispatch, and worker boundaries.
- Keep crash-prone native image execution outside the VM process while
  translating worker termination into an ordinary VM error.
- Admit native images before execution and bind each worker session to the
  embedded descriptor digest through bounded binary control frames.
- Provide tested adapter functions for standard-library runtime modules.

## Public Surface

- `native`: concrete Rust-backed adapters.
- `native_boundary`: handle-based bridge, dispatch, and worker runtime.
- `vm::pure_native`: transitional loader for the first direct-AOT scalar lane.

## Integration Points

- Native artifact execution reaches runtime services through typed boundaries,
  not through generated Rust application code or an Erlang backend.
- `commands::serve`: uses native HTTP/runtime adapters for development server
  behavior.

## Testing Notes

- Test native adapters directly and through NativeBoundary dispatch.
- Add adversarial tests for stale handles, wrong arity, and wrong resource kind.
- Decode generated native application failures into ordinary
  `std.core.Error.Error` records inside `Result`; reserve protocol failures for
  malformed replies and failed boundary transport.
