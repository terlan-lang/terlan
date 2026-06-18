# Terlan Erlang Core Emit Internals

This directory owns CoreIR-oriented Erlang lowering helpers. It is the backend
side of the formal compiler path and should grow as executable CoreIR replaces
syntax-output bridge emission.

## Responsibilities

- Lower CoreIR expressions, patterns, scalar values, collections, and type
  values to Erlang backend structures.
- Keep CoreIR-to-BEAM behavior separate from syntax-output bridge lowering.
- Provide helper functions for Erlang-safe names, runtime calls, and BEAM
  process-related forms.
- Preserve backend-agnostic CoreIR assumptions at the Erlang boundary.

## Public Surface

- `expr`: expression lowering helpers.
- `patterns`: pattern lowering helpers.
- `collections`: list/map/set-oriented lowering helpers.
- `scalar` and `type_values`: scalar/type value lowering.
- `runtime`, `beam`, and `helpers`: shared CoreIR backend helpers.

## Core Model

Core emit receives already-lowered CoreIR structures and maps them to Erlang
backend constructs. It should not recover source syntax meaning or perform
semantic checks that belong in type checking.

The main flow is:

1. Receive CoreIR node data from backend entry points.
2. Select the Erlang representation for the CoreIR operation.
3. Render or return backend structures to the parent emit layer.

Important invariants:

- CoreIR remains backend-agnostic before this boundary.
- Erlang-specific runtime helpers are introduced only in this backend.
- Unsupported CoreIR forms should fail with clear backend diagnostics.

## Integration Points

- `terlan_typeck::core_ir`: defines CoreIR structures and schema.
- Parent `emit`: coordinates module-level rendering.
- `runtime`: emits backend runtime helper calls as needed.

## Edge Cases

- Mutable receiver semantics must lower without changing Terlan value rules.
- Core collection operations must preserve target-neutral behavior.
- BEAM-specific process forms must remain isolated from portable CoreIR.

## Types And Interfaces

`expr`
: CoreIR expression lowering module.

`patterns`
: CoreIR pattern lowering module.

`helpers`
: Shared Erlang backend helper functions.

## Testing Notes

- Core emit tests live in adjacent `*_test.rs` files under `emit/`.
- Add exact tests when a CoreIR form gains executable Erlang lowering.
- Keep syntax-output bridge tests separate from CoreIR lowering tests.
