# Import Lint Tests

This directory owns import-related lint tests.

## Responsibilities

- Cover redundant import diagnostics.
- Cover unused import diagnostics.
- Cover selected default import diagnostics.

## Invariants

- Import diagnostics must preserve source spans and suggested intent.
- Type imports and value imports must remain distinct.
- Selected-default import rules must not collapse valid aliases.

## Testing Notes

- Nested modules mirror the import rule submodules.
