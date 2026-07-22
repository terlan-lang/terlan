# Standard Runtime Profile Internals

This directory owns focused operation tables for standard-library runtime
families whose support depends on the selected target profile.

## Responsibilities

- Classify supported `std.http.Response` operations by function and arity.
- Keep operation-level exceptions separate from module-family classification.
- Reject unknown operations instead of treating a supported module as fully
  portable.

## Integration Points

- `validation::target_profile::std_runtime`: validates imports and remote calls.
- `validation::target_profile::core_traversal`: applies operation decisions to
  typed CoreIR summaries.
- `std.http`: defines the source-facing response API.

## Testing Notes

- Profile tests cover every accepted operation and adversarial arity mismatch.
- New runtime operations must be classified explicitly before release.
