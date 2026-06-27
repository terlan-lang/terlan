# SafeNative Dispatch Internals

This directory owns dispatch helpers for SafeNative calls. The implementation is
centered on typed native operation selection. Its most important boundary is
that native execution remains behind validated compiler/runtime contracts.

## Responsibilities

- Route SafeNative requests to approved Rust implementations.
- Keep native dispatch names stable for generated code.
- Reject unsupported operations without unsafe fallback behavior.

## Public Surface

- `mod.rs`: SafeNative dispatch entry points.

## Core Model

Dispatch treats native operations as an explicit table of capabilities rather
than dynamic reflection.

The main flow is:

1. Receive a native operation request.
2. Match the operation against supported dispatch entries.
3. Execute or report a stable unsupported-operation error.

Important invariants:

- Dispatch must not call unregistered native code.
- Native errors must cross the bridge as typed failures.
- Operation names are part of the compiler/runtime contract.

## Integration Points

- `terlan_safenative`: owns native runtime surfaces.
- Compiler backends: emit calls that must match dispatch entries.

## Testing Notes

- Add dispatch tests for each new native capability.
