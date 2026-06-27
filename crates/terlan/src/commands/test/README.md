# Test Command Internals

This directory owns `terlc test`, the command that discovers Terlan `@test`
declarations and executes them against the selected target platform. The
implementation is centered on a formal-pipeline compile followed by a
target-owned runtime runner. The command must not invent a host-side test model
that bypasses CoreIR or backend emission.

## Responsibilities

- Parse command-local test flags such as `--target erlang`, `--target js`,
  `--name <test_function>`, `--emit-test-manifest <path>`, and
  `--emit-test-result-manifest <path>`.
- Compile the source module through the formal compiler pipeline.
- Discover and validate `@test` function declarations from syntax output.
- Emit, compile, and execute backend artifacts for supported runtime runners.
- Validate JS-target tests through the formal JS profile path until runtime JS
  execution is promoted.

## Public Surface

- `run`: command-router entry point used by `main.rs`.

No helper is public outside this module unless a neighboring command needs the
same behavior. Private helpers still document their inputs, outputs, and
transformations.

## Core Model

The command treats Terlan tests as normal source declarations with metadata.
Test discovery happens after formal syntax parsing and before target artifact
execution or target validation. The Erlang runner currently compiles emitted Erlang modules into a
temporary BEAM path, adds test-only exports for discovered tests, emits a
backend-owned EUnit wrapper, and invokes zero-argument test functions directly
for stable Terlan-facing output. Passing runs also validate the generated EUnit
wrapper silently.

The JS runner is validation-only in 0.0.4. It compiles the selected test module
through the JS target profile, validates `@test` declarations, and emits the
same manifest/result artifact shape as runtime runners. It does not execute
browser, worker, Node, or Oxc runtime code yet, and its output marks tests as
`ok (validated)`.

The command accepts either no path, one test file, or one test directory. With
no path, `terlc test` uses the project `tests` directory. Directory runs
discover `*Test.terl` files recursively in deterministic order. `--name`
selects one exact `@test` function after discovery; this is the compiler-backed
contract used by editor integrations for individual test runs. Manifest output
flags are single-file only until an aggregate manifest format is promoted.

The main flow is:

1. Parse the optional source path and optional target selector.
2. Compile the source through formal syntax, HIR, typecheck, and CoreIR phases.
3. Validate annotated test declarations.
4. Optionally emit a source-level test discovery manifest for release gates and
   runner integrations.
5. For runtime runners, emit the module and required test support modules into
   a temporary directory.
6. For runtime runners, compile and execute target artifacts.
7. Optionally emit a source-level test result manifest with pass/fail outcomes.

Important invariants:

- `terlc test` must execute target artifacts, not Rust-side expression
  simulation.
- Unsupported targets fail before execution or validation with an explicit
  diagnostic.
- `--target js` defaults to `js.shared` when no global JS profile is selected;
  explicit `js.browser` and `js.worker` profiles are preserved.
- The BEAM runner accepts public and module-local zero-argument tests. Private
  tests are exported only in the temporary test artifact and are not added to
  normal production emits.
- The opt-in test manifest records source path, Terlan module name, selected
  target, selected target profile, discovered test names, and source spans. It
  is a compiler/runner artifact, not a replacement for normal test output.
- The opt-in test result manifest records the same source/target identity plus
  pass/fail counts, per-test statuses, failure messages, and source spans.

## Integration Points

- `formal_pipeline`: compiles source through the canonical compiler path.
- `terlan_erlang`: emits Erlang from CoreIR plus syntax bridge data.
- `commands::artifacts`: collects imported file, template, and markdown inputs.
- `beam_runner`: owns temporary BEAM workspace cleanup, Erlang emission,
  `erlc` compilation, EUnit wrapper validation, and direct BEAM test execution.
- `manifest`: owns source-level test manifest JSON, result manifest JSON, and
  in-memory pass/fail report shapes.
- `release_support`: owns the embedded std support inventory and selection
  logic for installed `terlc test` runs.
- Erlang runtime tools: `erlc` builds BEAM artifacts and `erl` executes tests.
- Target profiles: JS validation uses `js.shared`, `js.browser`, or `js.worker`
  profile checks without runtime artifact execution.
- Release support modules: the first runner compiles the selected embedded
  0.0.1 `std/test` and `std/core` support modules into the temporary BEAM
  workspace so installed `terlc test` works outside the compiler repository.
- EUnit: the runner generates a backend-owned wrapper module for passing-run
  validation. This wrapper is not Terlan source syntax and is not a public
  standard-library API.

## File Layout

- `mod.rs`: command argument parsing, formal compilation, test discovery,
  target runner dispatch, and directory traversal.
- `beam_runner.rs`: temporary BEAM workspace ownership, emitted Erlang
  compilation, test-only exports, EUnit wrapper generation, and direct test
  execution.
- `command_runner.rs`: OS command execution helpers used by the runtime runner.
- `manifest.rs`: serializable manifest/result artifacts plus in-memory
  pass/fail report construction and rendering helpers.
- `release_support.rs`: embedded std release support modules and dependency
  selection for installed test execution.

## Edge Cases

- No discovered `@test` declarations is a command failure for now because the
  command is intended to validate a test-bearing module.
- A test returning `false` fails the run.
- A test returning anything other than `true` or `false` fails the run with the
  returned Erlang term shown.
- Missing `erlc` or `erl` is reported as an execution failure, not as a compiler
  success.

## Destruction And Cleanup

Temporary BEAM workspaces are removed when the command finishes. If cleanup
fails, the command does not mask an earlier test failure.

## Types And Interfaces

`TestArgs`
: Parsed command-local arguments: one source path and one target runner.

`TestTarget`
: Supported target runner selector. `erlang` executes target artifacts. `js`
validates JS-profile test modules without runtime execution in 0.0.4.

`DiscoveredTest`
: Validated source-level test metadata needed to invoke a backend function and
report user-facing results.

`TestDiscoveryManifest`
: Serializable test-runner metadata emitted by `--emit-test-manifest`; owned by
`manifest.rs`.

`TestDiscoveryManifestEntry`
: Serializable metadata for one discovered source-level test.

`TestResultManifest`
: Serializable test-runner result metadata emitted by
`--emit-test-result-manifest`; owned by `manifest.rs`.

`TestResultManifestEntry`
: Serializable execution result for one discovered source-level test.

`TestRunReport`
: In-memory aggregate pass/fail report produced by direct BEAM execution or
JS validation; owned by `manifest.rs`.

`TestRunResult`
: In-memory result for one executed source-level test.

`TestRunStatus`
: Stable pass/fail status vocabulary for result artifacts.

`TempBeamWorkspace`
: Temporary directory owner used for emitted `.erl` files and compiled `.beam`
artifacts; owned by `beam_runner.rs`.

`ReleaseSupportModule`
: Embedded `.terl` source module compiled into temporary test workspaces when
runtime tests need std support; owned by `release_support.rs`.

## Testing Notes

- Unit tests cover argument parsing, annotation detection, return-type checks,
  test-only export injection, EUnit wrapper rendering, manifest serialization,
  and Erlang atom quoting.
- Integration validation should run `terlc test --target erlang` and
  `terlc test --target js` against release fixtures after changes to emission,
  profile validation, or the runner. The
  `formal-0-0-1-test-runner-manifest-check` gate protects manifest metadata,
  `formal-0-0-1-test-runner-result-manifest-check` protects result metadata,
  and `formal-0-0-1-test-model-behavior-check` exercises the EUnit wrapper on
  passing fixtures.
