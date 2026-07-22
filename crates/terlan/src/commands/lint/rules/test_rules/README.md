# Test Lint Rules

This directory owns lint rules for Terlan test code.

## Responsibilities

- Detect weak or dishonest tests.
- Encourage property and table-driven testing where appropriate.
- Keep test-quality diagnostics separate from runtime test execution.

## Invariants

- Test lints must not reject meaningful minimal fixtures.
- Rule diagnostics must describe why a test body is weak.
- Property-test guidance should stay compatible with `std.test`.

## Testing Notes

- `property.rs` owns property-test-specific lint behavior.
