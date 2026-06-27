# CLI `emit` Command Internals

This directory owns the `terlc emit` command runtime. The implementation in
`mod.rs` is centered on the formal compiler path, backend-agnostic dependency
tracking, Erlang output emission, and interface/dependency manifest writes.

## Responsibilities

- Validate `emit` command-local arguments.
- Compile a source module through the formal compiler phases.
- Emit Erlang source, optional HRL headers, interface files, and dependency
  manifests.
- Emit native artifacts when source policy and source contents require them.
- Preserve incremental write, cache-dir, no-emit, and diagnostic behavior.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

Public methods or values exposed to callers include `run`.

## Core Model

The command is the standard file-to-compiler-output path. It compiles one
Terlan source file through syntax, resolution, and typecheck, then writes
backend output and compiler metadata needed by downstream incremental checks.

The main flow is:

1. Validate exactly one source path argument.
2. Read the source and compile through formal phases.
3. Respect `--no-emit` after validation.
4. Prepare output and cache directories.
5. Collect file, template, markdown, and interface dependency inputs.
6. Emit Erlang, HTML runtime when needed, HRL headers, interface text, and
   dependency manifests.

Important invariants:

- `--no-emit` still validates source through compiler phases.
- Dependency manifests are tied to the current syntax contract identity.
- Interface and dependency outputs are mirrored to cache-dir when configured.
- Native artifacts are emitted only when source uses native declarations.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState`, then transfers ownership to
`run`. The command performs one synchronous compile and output-write pass.

## Scheduling And Ordering

- Source read happens before compiler phase execution.
- Output directories are created only after source validation succeeds.
- Native artifacts are emitted before Erlang/interface outputs.
- Dependency manifests are computed before backend emission.

## Data Structures

- `CliCommand`: command-local path input.
- `CliState`: global output, cache, diagnostic, no-emit, incremental, and native
  policy settings.
- `DependencyManifest`: incremental dependency fingerprint record.
- CoreIR: formal backend handoff used to gate Erlang emission.
- Formal syntax output: temporary bridge input still used by Erlang and HRL
  lowering while CoreIR expression payloads are expanded; interface and
  dependency output still derive from checked compiler artifacts.

## Integration Points

- `main.rs`: routes the command and currently owns shared compiler helper
  functions.
- `terlan_erlang`: emits Erlang modules and HRL headers.
- Native policy validation: detects and emits safe native artifacts.
- Phase manifest validation: supplies the syntax contract identity.

## Edge Cases

- Missing or extra paths return exit code `2`.
- Read, compile, dependency input, emit, and write failures return exit code `1`.
- Cache-dir creation failures return exit code `1`.
- Empty HRL output is intentionally not written.

## Destruction And Cleanup

The command opens no long-lived resources. Temporary source/output strings,
dependency inputs, and manifests are dropped when `run` returns.

## Types And Interfaces

`CliCommand`
: Command-local source path container created by the top-level parser.

`CliState`
: Global CLI state used for compile policy and output behavior.

`DependencyManifest`
: Incremental output record used to track source, interface, doc, syntax
contract, and dependency hashes.

## Testing Notes

- Existing focused `emit` tests still live in the large `main.rs` test module
  while helper extraction is pending.
- Add module-local tests once dependency and output helper services move out of
  `main.rs`.
