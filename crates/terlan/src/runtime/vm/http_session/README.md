# VM HTTP Session Internals

This directory owns diagnostic projection for VM HTTP session state. Session
storage and actor ownership remain in the parent module; this boundary keeps
inspection output stable and free of host implementation details.

## Responsibilities

- Convert session state into typed runtime diagnostics.
- Preserve stable field names and deterministic ordering.
- Avoid exposing internal pointers, locks, or host handles.

## Integration Points

- `runtime::vm::http_session`: owns session lifecycle and policy.
- VM inspector and support bundles consume the projected diagnostics.

## Testing Notes

- Parent HTTP-session tests cover diagnostic shape and lifecycle transitions.
- Add rejection coverage whenever a new internal field must remain private.
