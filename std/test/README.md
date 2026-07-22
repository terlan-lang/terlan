# Std Test Internals

This directory owns the source-level testing helpers used by Terlan tests. It
is intentionally small and exists so std and user tests can assert behavior in
Terlan source instead of relying on ad hoc host scripts.

## Responsibilities

- Provide assertion helpers for `terlc test`.
- Keep test code expressible in Terlan source.
- Support release API tests that validate public standard-library behavior.
- Avoid growing into a broad test framework before the CLI test runner is
  stable.

## Public Surface

- `std.test.Test`: assertion, table, and lifecycle helper module.
- `std.test.Gen`: deterministic generator helper module for property-style
  tests.
- `std.test.Shrink`: deterministic shrinker helper module for property-style
  tests.

## Core Model

Tests are normal Terlan source functions marked with `@test`. Assertion helpers
return `Bool` for the current runner contract, allowing test results to be
validated consistently across supported targets.

Deterministic generators are finite by design in the initial property-test
surface. That keeps replay behavior explicit while shrinking, seeds,
max-success limits, discard limits, and distribution diagnostics are added in
later slices.

The main flow is:

1. A test module imports `std.test.Test` and, when needed, `std.test.Gen`.
2. A `@test` function calls assertion helpers.
3. `terlc test` compiles and runs the test for the selected target.

Important invariants:

- Test helpers remain source-level Terlan APIs.
- Release API tests must validate user-visible behavior, not internal compiler
  implementation details.
- Host scripts should not replace tests that can be written in Terlan.

## Integration Points

- `terlc test`: discovers and executes `@test` functions.
- `tests/std/RELEASE_API_TESTS.tsv`: records release API test coverage.
- Adjacent `*Test.terl` files: own positive std behavior coverage.

## Edge Cases

- Assertion diagnostics should become richer without changing the source-level
  test function shape.
- Target-specific test execution should report stable error codes and messages.

## Types And Interfaces

`Test`
: Source-level assertion helper module.

`Gen`
: Source-level deterministic generator helper module.

`Shrink`
: Source-level deterministic shrinker helper module.

## Testing Notes

- `std/test/AssertionsTest.terl` validates the assertion helper surface.
- `std/test/GenTest.terl` and `std/test/PropertyTest.terl` validate the
  deterministic property-test base surface.
- `std/test/ShrinkTest.terl` validates the deterministic shrinker base
  surface.
- New std modules should add adjacent tests rather than relying only on release
  orchestration.
- Doctest-style examples belong to later CLI/docs work.
