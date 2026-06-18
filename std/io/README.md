# Std IO Internals

This directory owns portable input/output modules. The current surface includes
console output, file helpers, and path helpers.

## Responsibilities

- Provide stable source-level IO APIs.
- Keep host filesystem and console details behind target-owned operations.
- Preserve explicit error handling for file and path operations.
- Avoid leaking Erlang, Rust, or JavaScript IO module names into portable
  source.

## Public Surface

- `std.io.Console`: console printing helpers.
- `std.io.File`: file read/write helpers.
- `std.io.Path`: path manipulation helpers.

## Core Model

IO operations are side-effecting target operations. Source code calls portable
module functions while the selected backend owns the actual host API.

The main flow is:

1. Source imports the IO module it needs.
2. Type checking validates function signatures and return types.
3. The backend lowers to the selected host runtime operation.

Important invariants:

- Console and file APIs stay under `std.io`.
- File operations must expose failures through typed results or stable
  diagnostics.
- Portable IO APIs must not force one target runtime's module names into
  Terlan source.

## Integration Points

- BEAM backend: may lower console output to Erlang IO operations.
- Native backend: may lower file/path work to Rust libraries.
- `terlc test`: uses IO behavior in release API smoke tests.

## Edge Cases

- Path separators and normalization are target-sensitive.
- File APIs must distinguish missing paths, permission failures, and invalid
  encodings as the error model expands.

## Types And Interfaces

`Console`
: Portable console output module.

`File`
: Portable file operation module.

`Path`
: Portable path helper module.

## Testing Notes

- Positive tests live beside modules as `std/io/*_test.terl`.
- Host-dependent behavior should use temporary directories in compiler tests.
- Release examples should prefer console output for simple smoke tests.
