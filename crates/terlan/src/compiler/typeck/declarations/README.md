# Declaration Typechecking Internals

This directory owns focused declaration analyses shared by typechecking and
later lowering phases.

## Responsibilities

- Infer and validate declaration-level purity metadata.
- Keep user assertions stricter than inferred implementation behavior.
- Emit stable diagnostics when declared contracts are violated.

## Testing Notes

Changes require positive inference tests and adversarial side-effect tests.
