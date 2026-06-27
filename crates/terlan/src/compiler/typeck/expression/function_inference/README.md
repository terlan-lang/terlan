# Function Inference Internals

This directory owns function-expression inference helpers for type checking.

## Responsibilities

- Infer function and lambda parameter/result types.
- Keep callable inference separate from general expression dispatch.
- Preserve diagnostics for ambiguous or unsupported callable shapes.

## Public Surface

- Module-local helpers consumed by `compiler::typeck::expression`.

## Integration Points

- `compiler::syntax`: supplies function expression syntax.
- `compiler::typeck::type_system`: unifies inferred function types.

## Testing Notes

- Add tests for lambda inference, higher-order calls, and rejected ambiguous
  function values.
