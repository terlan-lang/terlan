# CLI `interface` Command Internals

This directory owns the `terlc interface` command runtime. The implementation in
`mod.rs` is centered on converting checked interface syntax output into the
serialized `.typi` interface artifact consumed by downstream compile phases.

## Responsibilities

- Parse command-local interface arguments.
- Read one or more `.terli` or `.terl` source files.
- Parse interface source through the formal syntax-output path.
- Validate handwritten declarations, their dependencies, and implementations
  with sibling contracts through the shared formal pipeline.
- Write deterministic `<module>.typi` and `<module>.typi.deps` files into the
  configured output directory.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

Public methods or values exposed to callers include `run`.

## Core Model

The command has no persistent state. `main.rs` owns global argument parsing and
passes command-local strings plus `CliState` to this module. This module owns the
interface-specific read, parse, conversion, and write flow.

The main flow is:

1. Collect and parse the source paths, rejecting duplicate module identities.
2. Load sibling contracts and dependencies adjacent to every source directory.
3. Expand inherited fields across the complete batch graph before resolving
   clients, regardless of input order or whether parents use `.terl` or `.terli`.
4. If handwritten contracts are present, validate all batch inputs before
   publishing.
5. Write summaries and dependency fingerprints from the admitted public graph.

Batches containing only uncontracted `.terl` files retain syntax-based extraction
for standard-library bootstrapping. Contracted implementations export only their
declared surface.
Validation failures leave previously published summaries and manifests intact.

Important invariants:

- Interface generation uses formal syntax output, not the AST adapter path.
- Parse errors are reported through the shared diagnostic renderer.
- Incremental writes preserve the existing `write_if_changed_or_forced`
  behavior.

## Lifecycle

`run` is invoked once per CLI process execution for the `interface` verb. It
creates the configured output directory on demand and does not retain state
after returning an exit code.

## Integration Points

- `main.rs`: routes `interface` and supplies `CliState`.
- `terlan_syntax`: parses interface source into formal syntax output.
- `terlan_hir`: converts syntax output into interface artifact text.
- `write_if_changed_or_forced`: preserves incremental write behavior.

## Edge Cases

- Missing arguments produce exit code `2`.
- Read, parse, declaration validation, conformance, output-directory, and write
  failures produce exit code `1`.
- Parse diagnostics respect the caller's selected diagnostic format.

## Testing Notes

- Module tests cover contract enforcement, invalid dependencies, cross-directory
  batch boundaries, and validation before publication. The broader CLI suite
  covers ordinary extraction and output errors.
