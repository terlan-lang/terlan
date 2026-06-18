# Syntax Contract Command Internals

This module owns execution for the `terlc syntax-contract` command. The
implementation in `mod.rs` is centered on converting command-local arguments
into a typed command plan and delegating canonical contract data to
`terlan_syntax`.

## Responsibilities

- Parse `syntax-contract` command-local flags.
- Emit the cached canonical syntax contract artifact or fingerprint.
- Check a stored artifact or fingerprint against the current compiler contract.
- Keep command output and exit-code behavior out of `main.rs`.

## Public Surface

- `run`: command entry point called by the top-level CLI router.
- `parse_syntax_contract_command`: typed argument parser used by command tests.
- `syntax_contract_command_output`: returns artifact JSON or fingerprint text.
- `syntax_contract_file_check`: returns structured artifact check results.
- `syntax_contract_file_matches_current`: test-only boolean match helper.

## Core Model

The command has no persistent state. It operates on command-local arguments,
optional output paths, and syntax contract artifacts supplied by
`terlan_syntax`.

The main flow is:

1. `main.rs` routes the `syntax-contract` verb to `run`.
2. `run` parses command-local arguments into `SyntaxContractCommand`.
3. Emit mode loads cached contract output and writes stdout or a file.
4. Check mode reads an artifact and compares it to the current contract.

Important invariants:

- `--check` is mutually exclusive with `--fingerprint` and `--out`.
- `--fingerprint` only affects emit mode.
- File emission appends one trailing newline.
- Every function, including private helpers, documents its inputs, output, and
  transformation behavior.

## Integration Points

- `main.rs`: routes command arguments and prints global usage for parse errors.
- `terlan_syntax`: owns the canonical syntax contract artifact and comparison
  logic.
- Formal gates: use emitted/checkable contract artifacts to protect the compiler
  syntax boundary.

## Edge Cases

- Unknown or incomplete flags produce a usage error and exit code `2`.
- Invalid artifacts produce exit code `1`.
- Fingerprint mismatch reports expected and found fingerprint values.

## Testing Notes

- `run_syntax_contract_outputs_artifact_json` protects artifact, fingerprint,
  file output, check, mismatch, invalid artifact, and argument error behavior.
- Add focused tests when adding new command-local flags because invalid
  combinations are intentionally rejected in the parser.
