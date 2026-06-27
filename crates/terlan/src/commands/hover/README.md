# CLI `hover` Command Internals

This directory owns the `terlc hover` command runtime. The implementation in
`mod.rs` is centered on command-local argument handling, source validation, and
hover result selection.

## Responsibilities

- Parse `hover` command-local file, line, and column arguments.
- Read, parse, resolve, and typecheck the target Terlan source.
- Convert the requested line/column into a source byte offset.
- Print the first available hover result from type and documentation providers.
- Preserve existing output and exit-code behavior.

## Public Surface

- `run`: command entry point called by the top-level CLI router.
- Command-local helper functions are exposed as `pub(crate)` only where the
  existing CLI test suite still imports them directly.

Public methods or values exposed to non-test callers include `run`.

## Core Model

The command treats hover as a validated compiler query. It does not answer from
raw syntax alone; it first parses, resolves imports/interfaces, and rejects
type errors before producing hover output.

The main flow is:

1. Parse the target path and one-based source position.
2. Read and parse the module.
3. Load sibling interfaces, resolve names, and typecheck the module.
4. Convert the requested line/column into an offset.
5. Check component props, record fields, local docs, and imported docs.

Important invariants:

- Hover output must not be produced for modules with type errors.
- Position errors return exit code `2`.
- Missing hover information returns exit code `1`.
- The top-level router supplies global diagnostic formatting.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState` values, then transfers ownership
to `run`. The command performs one synchronous source query and returns an exit
code. It owns no persistent process state, background tasks, or caches.

## Scheduling And Ordering

- Argument parsing and source reads run before any compiler work.
- Parse diagnostics are emitted before HIR or typecheck work begins.
- Type errors are emitted before hover lookup and prevent hover output.
- Hover providers are checked in a stable order: component props, record fields,
  local docs, imported docs.

## Data Structures

- `CliCommand`: carries the command-local path, line, and column strings.
- `CliState`: carries global diagnostic formatting.
- `SyntaxModuleOutput`: formal syntax-output used by hover helper lookups.
- Interface map: sibling/imported module interfaces loaded for resolution and
  imported documentation lookup.

## Integration Points

- `main.rs`: routes the command and owns shared diagnostic helpers.
- Local parser helper: parses with `parse_module_as_syntax_output` / 
  `parse_interface_module_as_syntax_output`, returning `SyntaxModuleOutput`.
- `terlan_hir`: loads interfaces and resolves module names.
- `terlan_typeck`: rejects type errors before answering hover queries.
- Parser/diagnostic helpers: read and parse source and report diagnostics.

## Edge Cases

- Bad argument counts or non-numeric positions print usage and return `2`.
- Positions outside the source return `2`.
- Parser and typechecker failures emit diagnostics and return `1`.
- When no hover provider matches the requested offset, the command returns `1`.

## Destruction And Cleanup

The command opens no long-lived resources. File contents, parsed modules,
interfaces, and diagnostics are dropped when `run` returns.

## Types And Interfaces

`CliCommand`
: Command-local input container created by the top-level parser.

`CliState`
: Global CLI state used here for diagnostic formatting.

`SyntaxModuleOutput`
: Formal syntax-output module used as the hover query source.

## Testing Notes

- Existing focused hover tests still live in the large `main.rs` test module
  while test extraction is pending.
- Keep helper visibility narrow. Use `pub(crate)` only for helpers still called
  by the current test module.
