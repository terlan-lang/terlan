# Debug Command Internals

This directory owns native-image admission and source inspection for
`terlc debug`.

## Responsibilities

- Parse debugger command arguments and scripted debugger input.
- Model breakpoints and command scripts without executing user code while
  parsing.
- Admit compiler-generated `.tvm` images through `PureNativeExecutionShard`.
- Decode the native descriptor, continuations, exports, and source records.
- Resolve module/function and file/line breakpoints before live execution.
- Keep debugger diagnostics stable for CLI and editor integrations.

## Public Surface

- `mod.rs`: command entry point wiring.
- `breakpoint.rs`: breakpoint selector parsing and validation.
- `script.rs`: scripted debugger command parsing.
- `session.rs`: native-image admission and breakpoint resolution.

## Invariants

- Debugger parsing must not depend on host logs or panic output.
- Invalid breakpoint and script input must report typed diagnostics.
- The debugger rejects non-native targets, missing source metadata, target
  mismatches, and unresolved breakpoints.
- Opening a session uses the execution-shard admission path shared by run,
  test, REPL, and HTTP consumers.
- Live stepping and expression evaluation stay behind future VM-owned debug
  APIs; the command reports only metadata it has admitted.

## Testing Notes

- `debug_test.rs` owns command-level regression tests for debugger parsing and
  diagnostics.
- `session_test.rs` owns source-line and breakpoint-resolution tests.
