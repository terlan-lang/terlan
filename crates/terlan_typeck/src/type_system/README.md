# Type System Internals

This directory owns focused type-system helpers that are too specific for the
root `type_system.rs` module. The implementation currently separates interface
loading and lookup support from the general type model.

## Responsibilities

- Keep interface-specific type-system logic isolated.
- Preserve stable type lookup behavior for imports and generated summaries.
- Support future extraction from the root type-system module.
- Avoid backend-specific assumptions in type construction.

## Public Surface

- `interface`: helpers for interface and summary-backed type information.

## Core Model

The type system owns source-level type identities, aliases, constructors,
traits, and imported interfaces. Submodules here isolate narrower concerns so
the root module can remain an orchestration layer.

The main flow is:

1. Load local and imported type/interface declarations.
2. Resolve source names into type-system entries.
3. Provide checked type information to expression and lowering phases.

Important invariants:

- Imported interface data must be deterministic and target neutral.
- Type names and constructor names remain distinct where the language requires.
- Backend-only capabilities must be represented as target validation, not core
  type-system shortcuts.

## Integration Points

- `crate::type_system`: root type-system API.
- `crate::signature_loading`: summary and interface loading.
- `crate::expression`: consumes resolved types during checking.

## Edge Cases

- Missing imports should point at the import site when possible.
- Generated summaries must not introduce ambiguous type identities.
- Platform-specific wrappers require explicit target/profile metadata.

## Types And Interfaces

`interface`
: Interface and summary integration helpers for the type system.

## Testing Notes

- Type-system behavior is covered by adjacent `type_model_test.rs`,
  import tests, and std contract tests.
- Changes to import/interface resolution need both positive and diagnostic
  coverage.
- Generated summary behavior should remain byte-for-byte deterministic.
