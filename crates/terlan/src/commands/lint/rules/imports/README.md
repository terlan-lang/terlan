# Import Lint Rules

This directory owns lint rules for import declarations.

## Responsibilities

- Detect redundant import declarations.
- Detect unused import declarations.
- Detect selected default import shapes that should be simplified.

## Invariants

- Alias-preserving imports must not be rewritten by diagnostics.
- Type and value import namespaces must remain separate.
- Diagnostics must point to the import declaration that should change.

## Testing Notes

- Matching tests live under `lint_test/imports_test/`.
