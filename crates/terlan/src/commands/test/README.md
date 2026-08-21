# Test Command Internals

This directory owns `terlc test`, the command that discovers Terlan `@test`
and `@benchmark` declarations and executes them against the selected target platform. The
implementation is centered on a formal-pipeline compile followed by the
compiler-owned VM runner, hosted Wasm runner, or JS profile validation. The
command must not invent a host-side test model that bypasses CoreIR.

## Responsibilities

- Parse command-local test flags such as `--target terlan-vm`, `--target js`,
  `--target wasm`,
  repeated `--name <test_function>` selectors, `--emit-test-manifest <path>`, and
  `--emit-test-result-manifest <path>`.
- Select VM-native `@benchmark` declarations with `--bench`, with optional
  `--warmup <count>` and `--samples <positive-count>` controls.
- Compile the source module through the formal compiler pipeline.
- Discover and validate `@test` function declarations from syntax output.
- Execute supported tests through the Terlan VM runner.
- Emit checked Wasm artifacts and execute boolean test exports through the
  maintained hosted runtime.
- Validate JS-target tests through the formal JS profile path until runtime JS
  execution is promoted.

## Public Surface

- `run`: command-router entry point used by `main.rs`.

No helper is public outside this module unless a neighboring command needs the
same behavior. Private helpers still document their inputs, outputs, and
transformations.

## Core Model

The command treats Terlan tests as normal source declarations with metadata.
Test discovery happens after formal syntax parsing and before VM execution or
target validation. The public runtime runner is the compiler-owned Terlan VM
lane.

The JS runner is validation-only in 0.0.4. It compiles the selected test module
through the JS target profile, validates `@test` declarations, and emits the
same manifest/result artifact shape as runtime runners. It does not execute
browser, worker, Node, or Oxc runtime code yet, and its output marks tests as
`ok (validated)`.

The command accepts no path or one or more explicit test files and directories.
With no path, `terlc test` uses the project `tests` directory. Each directory
discovers `*Test.terl` files recursively in deterministic order. A multi-path
request retains a separate dependency session and compilation result per path
while amortizing compiler process startup; failure in one path does not prevent
the remaining paths from reporting their results. Repeated `--name` flags
select an exact `@test` subset after discovery and compile one shared source and
native application closure. Manifest output flags require exactly one path
until an aggregate manifest format is promoted.
`--bench` changes the selection category to `@benchmark`; benchmarks are not
run by ordinary test commands and currently execute only on `terlan-vm`.

The main flow is:

1. Parse the optional source paths and optional target selector.
2. Compile each source through formal syntax, HIR, typecheck, and CoreIR phases.
3. Validate annotated test declarations.
4. Optionally emit a source-level test discovery manifest for release gates and
   runner integrations.
5. For the VM runner, merge reachable project, standard-library, and test
   CoreIR at compile time, emit one application-native image, discard
   executable compiler IR, and execute every selected zero-arity test through
   that admitted image.
6. For the Wasm runner, emit the same artifact contract as `terlc build` and
   execute selected boolean exports through `terlc run` machinery.
7. Optionally emit a source-level test result manifest with pass/fail outcomes.

Important invariants:

- `terlc test` must execute through the Terlan VM, not through generated
  Erlang/BEAM artifacts.
- Unsupported targets fail before execution or validation with an explicit
  diagnostic.
- `--target js` defaults to `js.shared` when no global JS profile is selected;
  explicit `js.browser` and `js.worker` profiles are preserved.
- `--target wasm` selects `wasm.core`; missing hosted runtimes and unsupported
  ABI signatures fail rather than being recorded as validation-only passes.
- The VM runner accepts public zero-argument tests and reports unsupported
  runtime forms through stable VM diagnostics. Scalar, managed, and mixed
  selections all use `PureNativeExecutionShard`; no selection constructs an
  evaluator or retains executable CoreIR.
- Benchmark warmup and sample calls reuse the admitted AOT image and VM shard.
  Timing excludes compilation, image loading, and warmup, and reports native
  minimum, median, and p95 nanoseconds. Every sample must still return `true`,
  so performance cases retain a correctness assertion.
- The opt-in test manifest records source path, Terlan module name, selected
  target, selected target profile, discovered test names, and source spans. It
  is a compiler/runner artifact, not a replacement for normal test output.
- The opt-in test result manifest records the same source/target identity plus
  pass/fail counts, per-test statuses, failure messages, execution nanoseconds,
  and source spans. Execution timing excludes compilation and image loading.

## Integration Points

- `formal_pipeline`: compiles source through the canonical compiler path.
- `commands::artifacts`: collects imported file, template, and markdown inputs.
- `runtime::vm`: owns compiler-checked test execution for the public VM lane.
- `commands::wasm_runtime`: owns hosted Wasm execution and result validation.
- `manifest`: owns source-level test manifest JSON, result manifest JSON, and
  in-memory pass/fail report shapes.
- Target profiles: JS validation uses `js.shared`, `js.browser`, or `js.worker`
  profile checks without runtime artifact execution.

## Native-Only Gate

`make tvm-aot-test-consumer-check` executes passing, failing, managed/mixed,
and manifest-producing test selections, verifies one application-image build
and admitted shard load, and rejects evaluator or serialized-runtime symbols
from production test-command sources.

## File Layout

- `mod.rs`: command argument parsing, formal compilation, test discovery,
  target runner dispatch, and directory traversal.
- `manifest.rs`: serializable manifest/result artifacts plus in-memory
  pass/fail report construction and rendering helpers.
- `process_runner.rs`: generic bounded process execution helpers used by
  test-only reference runners.

## Edge Cases

- No discovered `@test` declarations is a command failure for now because the
  command is intended to validate a test-bearing module.
- A test returning `false` fails the run.
- A test returning anything other than `true` or `false` fails the run with a
  stable test-result diagnostic.

## Destruction And Cleanup

Temporary test workspaces are removed when the command finishes. If cleanup
fails, the command does not mask an earlier test failure.

## Types And Interfaces

`TestArgs`
: Parsed command-local arguments: one source path and one target runner.

`TestTarget`
: Supported target runner selector. `terlan-vm` executes VM artifacts, `wasm`
executes Wasm artifacts, and `js` validates JS-profile test modules without
runtime execution.

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
: In-memory aggregate pass/fail report produced by VM execution or
JS validation; owned by `manifest.rs`.

`TestRunResult`
: In-memory result for one executed source-level test.

`TestRunStatus`
: Stable pass/fail status vocabulary for result artifacts.

## Testing Notes

- Unit tests cover argument parsing, annotation detection, return-type checks,
  manifest serialization, and test result rendering.
- Integration validation should run `terlc test`, `terlc test --target wasm`,
  and `terlc test --target js`
  against release fixtures after changes to profile validation or the runner.
  The
  `formal-0-0-1-test-runner-manifest-check` gate protects manifest metadata,
  `formal-0-0-1-test-runner-result-manifest-check` protects result metadata,
  and `formal-0-0-1-test-model-behavior-check` exercises VM behavior on
  passing fixtures.
