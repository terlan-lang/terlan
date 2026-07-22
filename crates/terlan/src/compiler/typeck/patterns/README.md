# Pattern Typechecking Internals

This directory owns focused pattern typechecking helpers that are too large for
the parent pattern dispatcher. It derives bindings and diagnostics from checked
pattern structure without changing parser output.

## Responsibilities

- Validate string-capture pattern structure and capture types.
- Preserve source spans for stable diagnostics.
- Return bindings through the parent typechecker contract.

## Core Model

`string_capture` checks literal and named segments against the expected string
type. It must reject malformed or duplicate captures before bindings enter the
surrounding case or function-head scope.

## Integration Points

- `compiler::typeck::patterns`: owns pattern dispatch and binding collection.
- `compiler::syntax`: supplies parsed pattern spans and capture segments.

## Testing Notes

- Pattern behavior is covered by adjacent parent `pattern_test.rs` and
  adversarial parser/typechecker suites.
- Add a focused positive and rejection test for every capture rule change.
