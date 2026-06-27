# Trait Model Internals

This directory owns the typed trait model used by conformance checks.

## Responsibilities

- Represent trait requirements, default methods, and implementation contracts.
- Keep trait text parsing separate from expression type checking.
- Provide stable structures for cross-platform std contracts.

## Public Surface

- Trait model helpers consumed by `compiler::typeck`.

## Integration Points

- `compiler::hir`: supplies resolved module and interface metadata.
- `compiler::typeck::trait_conformance`: validates implementations against the
  model.

## Testing Notes

- Add focused tests for defaults, generic arity, and implementation mismatch
  diagnostics.
