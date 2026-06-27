# Target Profile Shape Internals

This directory owns target-profile shape validation helpers.

## Responsibilities

- Reject CoreIR, pattern, and expression forms unsupported by a target profile.
- Keep profile-specific shape rules split by compiler artifact kind.
- Preserve stable diagnostics for target capability mismatches.

## Public Surface

- Module-local validators consumed by `validation::target_profile`.

## Integration Points

- `compiler::typeck`: supplies CoreIR and pattern structures.
- `validation::target_profile`: coordinates profile compatibility checks.

## Testing Notes

- Add tests for every newly accepted or rejected CoreIR shape.
- Keep JS, Erlang, native, and core-v0 profile expectations explicit.
