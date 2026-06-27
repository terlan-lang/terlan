# CLI `check` Command Internals

This directory owns the `terlc check` command runtime. The implementation in
`mod.rs` is centered on command-local argument parsing, single-file formal
compile validation, and directory/incremental check orchestration.

## Responsibilities

- Parse `check` command-local arguments.
- Run single-file checks through the formal syntax-output compiler pipeline.
- Run directory checks with interface-cache and dependency-manifest support.
- Emit optional phase manifests for single-file and directory paths.
- Preserve invalidation tracing output.

## Public Surface

- `run`: command entry point called by the top-level CLI router.
- `run_check_dir`: directory-check entry point used by focused tests while the
  large test module is still being split.
- `parse_check_args`: typed parser for `check` command-local arguments.

Public methods or values exposed to callers include `run`, `run_check_dir`, and
`parse_check_args`.

## Core Model

`main.rs` owns global arguments and routes `check` into this module. This module
owns check-specific flags and orchestration for both source files and source
directories. Shared compiler helpers still live outside this module until the
broader CLI internals are split further.

The main flow is:

1. Parse a source path and optional `--emit-phase-manifest` path.
2. For files, read source and compile through parse, macro expansion, resolve,
   and typecheck phases.
3. For directories, discover modules, refresh interface caches, and recheck
   changed dependency closures.
4. Emit phase manifests when requested.
5. Report invalidation tracing and return the command exit code.

Important invariants:

- `check` uses the formal `SyntaxModuleOutput` compile path.
- Phase manifests are validated before writing.
- Argument errors return exit code `2`; compile and I/O errors return exit code
  `1` or the propagated compile phase code.

## Integration Points

- `main.rs`: routes the command and supplies global CLI state.
- `validation::phase_manifest`: builds and writes phase manifests.
- Formal compile helpers: produce checked syntax artifacts and phase diagnostics.

## Edge Cases

- Read failures can still produce parse-error phase manifests when requested.
- Parse failures skip macro expansion, resolve, and typecheck phases in emitted
  manifests.
- Invalidation tracing reports interface cache hit/miss when a cache directory
  is configured.

## Testing Notes

- Existing `main.rs` tests still cover check behavior while the broader test
  module is being split.
- Add focused parser tests here when new check-local flags are introduced.
