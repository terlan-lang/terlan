# Target Profile Core Traversal Internals

This directory owns VM-specific exceptions used while recursively validating
typed CoreIR expression summaries. General traversal and diagnostic assembly
remain in the parent module.

## Responsibilities

- Recognize expressions implemented by VM-owned standard runtime operations.
- Inspect nested calls, clauses, and remote-call summaries conservatively.
- Keep VM allowances from weakening JS, WASM, or other target profiles.

## Integration Points

- `validation::target_profile::core_traversal`: performs recursive validation.
- `validation::target_profile::std_runtime`: classifies supported std calls.
- `terlan_typeck`: supplies typed CoreIR summaries and expressions.

## Testing Notes

- Target-profile tests cover accepted VM operations and rejected cross-target
  uses.
- Every new allowance requires an equivalent negative profile test.
