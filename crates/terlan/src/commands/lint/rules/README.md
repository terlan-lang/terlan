# Lint Rules

This directory owns concrete lint rule implementations.

## Responsibilities

- Group lint rules by semantic area.
- Keep each rule small enough to test independently.
- Return structured diagnostics instead of printing directly.

## Invariants

- Rules must be deterministic across filesystem traversal order.
- Rules must not mutate source text.
- Shared traversal helpers should live at the command layer, not inside one
  rule family.

## Testing Notes

- Each rule family should have matching fixtures under `lint_test/`.
