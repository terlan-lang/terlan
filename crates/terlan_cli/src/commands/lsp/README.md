# LSP Command Internals

This module owns execution for the `terlc lsp` command. The implementation in
`mod.rs` is centered on validating command-local transport flags and delegating
stdio execution to `terlan_lsp`.

## Responsibilities

- Execute `terlc lsp` and `terlc lsp --stdio`.
- Print command-local help for `terlc lsp --help`.
- Reject unsupported LSP command arguments with usage output.
- Keep LSP command routing out of `main.rs`.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

## Core Model

The module has no persistent state. It accepts command-local argument strings
and returns an `ExitCode`.

The main flow is:

1. `main.rs` routes the `lsp` verb to `run`.
2. `run` handles `--help`, no arguments, or `--stdio`.
3. Valid stdio requests delegate to `terlan_lsp::run_stdio_server`.
4. Invalid arguments print usage and return exit code `2`.

Important invariants:

- The only accepted runtime transport spelling is `--stdio`.
- No arguments default to stdio transport.
- `--help` does not start the server.
- Every function, including private helpers, documents its inputs, output, and
  transformation behavior.

## Integration Points

- `main.rs`: routes the `lsp` verb.
- `terlan_lsp`: owns the language-server stdio loop.

## Edge Cases

- More than one argument is rejected.
- Unknown single arguments are rejected and echoed in the error message.

## Testing Notes

- Add focused command tests before extending transport options.
- Full stdio behavior belongs in `terlan_lsp` tests rather than CLI routing
  tests.
