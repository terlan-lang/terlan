# Core Intrinsic Lowering Internals

This directory owns typecheck-side lowering for compiler-known core intrinsics.
The implementation is centered on translating approved intrinsic syntax into
typed CoreIR forms. Its most important boundary is that intrinsic handling must
remain explicit and narrow.

## Responsibilities

- Recognize approved core intrinsic calls after syntax parsing.
- Produce typed CoreIR-compatible expressions.
- Reject unsupported intrinsic shapes with stable diagnostics.

## Public Surface

- `mod.rs`: core intrinsic lowering entry points.

## Core Model

Core intrinsic lowering bridges source-level helper calls to compiler-owned
primitive operations.

The main flow is:

1. Inspect a typed syntax call.
2. Match it against approved intrinsic forms.
3. Emit the corresponding CoreIR primitive expression.

Important invariants:

- Intrinsics must not become a general escape hatch.
- Type information must be preserved for backend validation.
- Unsupported forms must remain ordinary calls or diagnostics.

## Integration Points

- `terlan_typeck`: supplies types and inference context.
- CoreIR lowering: consumes intrinsic primitive expressions.

## Testing Notes

- Add focused tests for every intrinsic mapping and unsupported shape.
