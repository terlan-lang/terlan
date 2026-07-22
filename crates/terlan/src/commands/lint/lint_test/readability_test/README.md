# Readability Lint Tests

This directory owns readability-focused lint tests.

## Responsibilities

- Cover branch readability diagnostics.
- Keep readability recommendations separate from mandatory syntax rules.
- Exercise opinionated style checks that formatter does not rewrite.

## Invariants

- Readability lints must remain deterministic and actionable.
- Diagnostics should name the readable replacement shape when one exists.
- Rules must not conflict with canonical formatter output.

## Testing Notes

- `branch_test` covers branch-specific readability regressions.
