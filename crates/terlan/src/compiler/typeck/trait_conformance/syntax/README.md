# Trait Conformance Syntax Internals

This directory owns syntax-facing helpers for trait conformance validation.

## Responsibilities

- Read trait and implementation declarations from syntax output.
- Normalize source-level trait requirements before semantic checks.
- Keep parser-shape handling out of trait model logic.

## Public Surface

- Module-local helpers consumed by `compiler::typeck::trait_conformance`.

## Integration Points

- `compiler::syntax`: supplies declaration and token structures.
- `compiler::typeck::trait_model`: owns semantic trait contracts.

## Testing Notes

- Add tests for syntax forms before adding semantic conformance rules.
- Keep malformed declaration diagnostics stable.
