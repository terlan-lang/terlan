# [terlan] Internals

This package owns the `terlc` binary and command dispatch layer. `main.rs` is
being reduced to process entry, global argument parsing, and routing; command
execution lives under `src/commands`, and reusable validation logic lives under
`src/validation`.

## Responsibilities

- Parse CLI flags and subcommands for the full Terlan workflow.
- Run checks/emission/doc/doctest/hover/adapters/machine checks.
- Coordinate output format, cache mode, and exit-code semantics.
- Invoke the syntax, HIR, type-checking, and Erlang backend feature modules.
- Provide user-facing diagnostics in text or JSON format.

## Public Surface

- `main`: binary entrypoint (`terlc`).
- `parse_args`: argument parser into `CliState` + command verb.
- `run_cli`: routes parsed command verbs to formal command modules and remaining
  compatibility handlers that have not yet been extracted.
- `commands::syntax_contract::run`: maintainer/release tooling that emits the
  validated compiler-facing syntax contract artifact JSON, or just its
  fingerprint with `--fingerprint`. `--out <path>` writes either form to a file;
  `--check <path>` verifies a saved artifact or fingerprint against the
  compiler. Normal end-user compile commands do not validate the EBNF contract
  per compile.
- `commands::checks::run_check_stdlib` / `commands::checks::run_check_adapter`:
  static contract check commands.
- `run_doc`, `run_doctest`: documentation and doc-test validation pipelines.

## Core Model

- `CliState`: runtime flags and output settings.
- `CliCommand`: parsed verb + positional args.
- `ColorChoice`, `DiagnosticFormat`, `DocFormat`, `NativePolicy`: behavioral switches.

Flow:

1. CLI args -> parse into `CliState`/`CliCommand`.
2. Dispatch by verb in `run_cli`.
3. Route to a command module or quarantined compatibility handler.
4. Return structured exit code and optionally emit files.

## Files

- `Cargo.toml`: CLI crate manifest and dependencies.
- `src/main.rs`: process entry, global argument parsing, command routing, and
  remaining compatibility command handlers during the split.
- `src/commands/`: extracted verb-specific command execution modules.
- `src/validation/`: extracted validation helpers shared by command workflows.

## Integration Points

- `parse_module_as_syntax_output`/`parse_interface_module_as_syntax_output`
  from the syntax feature on formal paths.
- `resolve_syntax_module_output_with_interfaces` and interface loading from
  the HIR feature.
- `type_check_syntax_module_output` from the type-checking feature on formal paths.
- CoreIR-gated Erlang emission and syntax-output header emission from the
  Erlang backend feature.
- Uses the HTML feature for documentation and static-site HTML helpers.

## Edge Cases

- Unknown commands/invalid options print usage and return exit code `2`.
- Malformed files or parse errors return error diagnostics immediately.
- Incremental mode relies on interface/cache fingerprints.
- Missing docs or doctest failures fail the command in `--check` paths.

## Cleanup

- No persistent process-level mutable state.
- Command-local maps/files/scopes are dropped when each handler returns.

## Testing Notes

- Tests are still mostly colocated in `src/main.rs` while the split is in
  progress. New modules should keep code below 1000 lines and move tests toward
  module-local coverage with a 2000-line test target.
- Regression-sensitive areas are argument parsing and cache invalidation behavior.
