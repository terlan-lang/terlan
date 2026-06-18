# CLI Command Internals

This directory owns command execution modules routed by `crates/terlan_cli/src/main.rs`.
The implementation is split by CLI verb so the top-level file can stay focused
on process entry, global argument parsing, and command routing.

## Responsibilities

- Keep verb-specific command execution out of `main.rs`.
- Group command-local argument parsing with the command that consumes it.
- Preserve existing exit-code and stderr/stdout behavior while extracting code.
- Keep command modules cohesive. Avoid creating many small modules only to meet
  a line-count target.

## Public Surface

- `artifacts`: shared dependency manifest, fingerprint, and import-loading helpers.
- `build`: executes `terlc build`.
- `syntax_contract`: executes `terlc syntax-contract`.
- `check`: executes `terlc check`.
- `fmt`: executes `terlc fmt`.
- `hover`: executes `terlc hover`.
- `init`: executes `terlc init`.
- `emit_js`: executes `terlc emit-js`.
- `test`: executes `terlc test`.
- `emit_native_metadata`: executes `terlc emit-native-metadata`.
- `interface`: executes `terlc interface`.
- `json`: shared JSON rendering helpers for command modules.
- `repl`: executes `terlc repl`.
- `static_site`: executes `terlc emit-static` and `terlc serve-static`.
- `doc`: executes `terlc doc` and `terlc doctest`.
- `emit`: executes `terlc emit`.

Command modules should expose a narrow `run` function for the top-level router.
Additional helpers may be `pub(crate)` only when tests or neighboring modules
need typed access to command-local behavior.

## Core Model

The top-level CLI parses global flags into `CliState` and command-local strings
into `CliCommand`. Command modules receive the minimal data needed for their
verb and return `ExitCode`.

The main flow is:

1. `main.rs` receives process arguments.
2. `main.rs` parses global flags and identifies the verb.
3. `main.rs` routes the verb to a command module.
4. The command module owns command-local parsing, execution, and status.

Important invariants:

- `main.rs` should remain a router, not a home for command implementation.
- Command modules do not own global argument parsing.
- Command modules document every function's inputs, output, and transformation
  behavior, including private helpers.
- Command module size is a guideline, not a hard limit. A cohesive command
  module can grow to roughly 1500 lines before splitting is expected.
- Split command modules only when there are clear subdomains, such as argument
  parsing, report rendering, runner/process execution, or reusable services.
- Keep tests focused on the command behavior. Larger command test files are
  acceptable when they avoid scattering one command's behavior across too many
  modules.

## Integration Points

- `main.rs`: routes CLI verbs to command modules.
- `validation`: contains reusable validation helpers that command modules may
  call.
- Compiler crates: command modules call syntax, HIR, typecheck, and backend
  APIs as needed for their verb.

## Testing Notes

- Move tests with command modules when the surrounding fixtures can move cleanly.
- Until the large test module is split, focused tests in `main.rs` may import
  command helpers directly.
- Run focused command tests after each extraction before broader CLI gates.
