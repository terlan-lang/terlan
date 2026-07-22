# Core Expression Lowering Internals

This directory owns focused expression lowering from typed syntax into CoreIR.

## Responsibilities

- Lower branching and error-control expressions without losing inferred types.
- Preserve source spans for downstream diagnostics.
- Reject forms whose control-flow invariants cannot be represented safely.

## Testing Notes

Cover success paths, type mismatches, nested control flow, and adversarial
unreachable branches.
