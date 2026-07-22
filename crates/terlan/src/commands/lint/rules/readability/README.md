# Readability Lint Rules

This directory owns lint rules that improve source readability without changing
language semantics.

## Responsibilities

- Detect branch and expression forms that are valid but harder to read.
- Provide actionable diagnostics for strict lint profiles.
- Stay separate from formatter-only normalization.

## Invariants

- Readability rules must not make syntax validity decisions.
- Suggested rewrites must preserve semantics.
- Rules must not fire on formatter-preferred code.

## Testing Notes

- Matching tests live under `lint_test/readability_test/`.
