# CLI `repl` Command Internals

This directory owns the `terlc repl` command runtime. The implementation is
centered on interactive session state, source-compatible entry parsing, seed
module/project loading, CoreIR evaluator execution, and structured REPL events.

## Responsibilities

- Parse `repl` command-local arguments.
- Create and clean a temporary REPL compiler source directory.
- Load optional seed file or project declarations.
- Accept interactive control commands, declarations, imports, persistent value
  bindings, and expressions.
- Emit text or JSON REPL events while preserving existing behavior.

## Public Surface

- `run`: command entry point called by the top-level CLI router.

Public methods or values exposed to callers include `run`.

## Core Model

The REPL maintains in-memory accumulated declarations and persistent value
bindings. It evaluates expressions by generating a temporary Terlan module,
compiling it through the formal compiler phases, then executing the generated
CoreIR function through the compiler-owned evaluator in `evaluator.rs`.

The main flow is:

1. Parse optional help or seed module path.
2. Create a generated session module name and temporary directory.
3. Load seed declarations if supplied.
4. Read one interactive line at a time.
5. Dispatch REPL control commands, declarations, imports, or expression
   execution.
6. Remove the temporary session directory when the session exits.

Important invariants:

- `:reset` clears both seed and session declarations.
- `:load path` accepts one `.terl` file or a project directory with
  `terlan.toml`; project directories load only manifest-declared source roots.
- Expressions are compiled through the same formal compiler path as files and
  executed through the CoreIR evaluator.
- REPL-only value entries use `let name = expr.` and persist as ordinary
  generated Terlan `let` bindings for later expression evaluation.
- Source-level type introspection uses implicit `type_of(value)` and
  `is_type(value, Type)` calls lowered through CoreIR intrinsics, not a REPL
  command.
- JSON mode emits stable `terlan-repl-event-v1` records.
- In JSON mode, console output effects are emitted as `stdout` events instead
  of raw lines so tooling can consume the REPL stream as newline-delimited JSON.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState`, then transfers ownership to
`run`. The REPL owns its temporary directory and session declaration list for
the lifetime of the interactive loop.

## Scheduling And Ordering

- Seed module parsing happens before the initial ready event.
- Prompt output happens before each blocking stdin read in text mode.
- Expression parse is attempted before declaration parse for ordinary input.
- Control commands are handled before source parsing.

## Data Structures

- Declaration vector: accumulated session declarations.
- Baseline declaration vector: declarations loaded from the current seed module.
- Generated module name: unique module identifier for temporary compiler input.
- Temporary directory: per-session compiler source workspace.

## Integration Points

- `main.rs`: routes the command and currently owns shared compiler helper
  functions.
- `terlan_syntax`: parses declarations, expressions, and formats generated
  modules.
- Formal compiler phases: compile generated REPL modules before evaluation.
- CoreIR evaluator: executes selected CoreIR expression and std effect hooks
  without invoking a target runtime.

## Edge Cases

- Extra command arguments return exit code `2`.
- Seed read or parse failures return exit code `1`.
- Unterminated source entries emit a normal parse-style REPL error.
- Successful declarations, imports, value bindings, loads, resets, and effecting
  expressions render `Unit`, not a host-specific status word.
- Built-in type values such as `Int`, `String`, `Bool`, and `Type` are available
  to `type_of`/`is_type` without imports. Library types remain import-owned.

## Destruction And Cleanup

The temporary REPL directory is removed on EOF and explicit quit. Cleanup errors
are reported and converted into exit code `1`.

## Types And Interfaces

`CliCommand`
: Command-local input container created by the top-level parser.

`CliState`
: Global CLI state used for diagnostics, native policy, and event format.

Declaration vector
: Session source state used to rebuild generated modules for each query.

Value-binding vector
: REPL-only session state entered with `let name = expr.` and lowered into
ordinary Terlan `let` expressions for subsequent evaluation.

## Testing Notes

- Existing REPL smoke tests live outside this module and exercise the CLI
  process boundary.
- Add module-local tests only when non-interactive helper behavior is split into
  public testable seams.
