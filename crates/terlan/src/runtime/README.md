# Runtime Internals

This directory owns Rust runtime adapters used by generated Terlan artifacts.

## Responsibilities

- Keep concrete Rust-native implementations separate from bridge policy.
- Own SafeNative request, resource, dispatch, and worker boundaries.
- Provide tested adapter functions for standard-library runtime modules.

## Public Surface

- `native`: concrete Rust-backed adapters.
- `safenative`: handle-based bridge, dispatch, and worker runtime.

## Integration Points

- `backends::erlang`: emits calls that reach SafeNative helpers.
- `commands::serve`: uses native HTTP/runtime adapters for development server
  behavior.

## Testing Notes

- Test native adapters directly and through SafeNative dispatch.
- Add adversarial tests for stale handles, wrong arity, and wrong resource kind.
