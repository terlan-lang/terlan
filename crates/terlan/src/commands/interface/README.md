# CLI `interface` Command Internals

This directory owns the `terlc interface` command runtime. The implementation in
`mod.rs` is centered on converting checked interface syntax output into the
serialized `.typi` interface artifact consumed by downstream compile phases.

## Responsibilities

- Parse command-local interface arguments.
- Read exactly one `.terli` source file.
- Parse interface source through the formal syntax-output path.
- Write a deterministic `<module>.typi` file into the configured output
  directory.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

Public methods or values exposed to callers include `run`.

## Core Model

The command has no persistent state. `main.rs` owns global argument parsing and
passes command-local strings plus `CliState` to this module. This module owns the
interface-specific read, parse, conversion, and write flow.

The main flow is:

1. Validate that exactly one source path was supplied.
2. Read the source file through the shared CLI file helper.
3. Parse `.terli` text with `parse_interface_module_as_syntax_output`.
4. Convert syntax output into HIR interface text and write `<module>.typi`.

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

- Missing or extra arguments produce exit code `2`.
- Read, parse, serialization, output-directory, and write failures produce exit
  code `1`.
- Parse diagnostics respect the caller's selected diagnostic format.

## Testing Notes

- Existing `main.rs` tests still cover interface success and error paths while
  the broader CLI test module is being split.
- Add focused module tests when new interface-local flags or output modes are
  introduced.
