# Build Command Test Internals

This directory owns focused build-command regression tests. The tests are split
by behavior area so build coverage can grow without turning one file into a
large mixed test module.

## Responsibilities

- Validate build argument parsing and project layout handling.
- Validate emitted artifacts for Terlan VM, JavaScript, browser packaging, and
  standard-library integration.
- Cover dependency, import, constructor, trait, std runtime, and diagnostic
  behavior.
- Keep tests adjacent to the build command while separate from implementation
  files.

## Public Surface

- `args_test.rs`: build argument behavior.
- `artifact_test.rs`: emitted artifact behavior.
- `executable_vm_artifact_test.rs`: VM executable artifact behavior without
  Erlang launcher/runtime coverage.
- `js_target_diagnostics_test.rs`: JavaScript target-family rejection
  diagnostics.
- `wasm_artifact_metadata_test.rs`: VM-default and reserved Wasm/WASI package
  metadata behavior.
- `std_runtime_test.rs`: removed public Erlang target rejection coverage.
- Dependency, project layout, imported-constructor, and target-family test
  files.

## Core Model

Each file owns one behavior cluster and creates temporary source/project
fixtures. Tests should validate public command behavior rather than private
helper internals when possible.

The main flow is:

1. Create a temporary Terlan project or source file.
2. Invoke build command helpers or the CLI path.
3. Assert diagnostics, artifacts, manifests, or runtime output.

Important invariants:

- Tests must not require network access.
- Tests must not require `erl`, `erlc`, generated Erlang source, or BEAM
  bytecode.
- Fixture output should be deterministic.

## Integration Points

- `commands::build`: build command implementation.
- JS emission paths and VM artifact checks.
- Standard-library summaries and manifests.

## Edge Cases

- Local path dependency cycles must fail clearly.
- Wrong target dependencies must be rejected before emission.
- Browser packaging must validate manifest paths and generated assets.

## Types And Interfaces

Build command tests use Rust test functions only; there is no public runtime
interface in this directory.

## Testing Notes

- Add new build tests to the narrowest existing behavior file.
- Create a new test file only when a new behavior cluster appears.
- Keep helper duplication low by extracting shared test support when repeated.
