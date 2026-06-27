# Erlang Syntax Intrinsics Internals

This directory owns syntax-bridge lowering for compiler-known intrinsic calls.
The implementation is centered on explicit intrinsic mappings. Its most
important boundary is that ordinary Terlan source should lower through typed
semantics, while intrinsic escapes remain narrowly owned here.

## Responsibilities

- Lower approved primitive and runtime intrinsic calls.
- Preserve backend-specific ABI details behind typed helper functions.
- Keep intrinsic mappings explicit and auditable.

## Public Surface

- `mod.rs`: intrinsic call routing for syntax-to-Erlang lowering.
- `http`: HTTP runtime intrinsic lowering.

## Core Model

Intrinsic lowering maps validated Terlan call shapes to Erlang runtime calls.

The main flow is:

1. Receive a typed syntax call candidate.
2. Match the intrinsic module/function shape.
3. Emit an Erlang expression with the expected runtime ABI.

Important invariants:

- New intrinsics must be compiler-owned, not user-visible magic.
- Backend details must not leak into canonical source syntax.
- Unsupported intrinsic shapes must decline cleanly.

## Integration Points

- `emit::syntax::calls`: routes intrinsic call candidates.
- Erlang runtime helpers: receive emitted backend calls.

## Testing Notes

- Add syntax-lowering tests when adding or changing intrinsic mappings.
