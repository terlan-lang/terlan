# CLI `doc` And `doctest` Command Internals

This directory owns the `terlc doc` and `terlc doctest` command runtimes. The
implementation in `mod.rs` is centered on command-local argument parsing,
documentation validation, output rendering, and doctest compilation.

Going forward, generated documentation must be a usable public documentation
artifact, not a validation artifact. `doc --check` may exist specifically for
compiler gates, but `doc` itself must produce output that a user can navigate
and read as a stdlib reference.

## Responsibilities

- Parse `doc` command-local path and validation flags.
- Validate documentation links, fences, and optional missing-doc requirements.
- Render Markdown or HTML documentation output that is usable as public
  reference documentation, not merely proof that extraction ran.
- Compile Terlan doctest fences for the `doctest` command.
- Extract and validate REPL prompt examples from `@example` documentation
  blocks for the REPL-backed doctest path.
- Render a compiler-owned JSON documentation model for downstream tools.
- Preserve existing output and exit-code behavior.

## Public Surface

- `run`: command entry point for `terlc doc`.
- `run_doctest`: command entry point for `terlc doctest`.
- `parse_doc_args`: typed parser for `doc` command-local arguments.
- `render`: Markdown and HTML documentation rendering from formal syntax
  output.
- `validation`: source discovery, doc link/fence validation, missing-doc
  validation, and Terlan doctest compilation.

Public methods or values exposed to callers include `run`, `run_doctest`, and
`parse_doc_args`.

## Core Model

Documentation commands run as compiler validation queries over syntax output.
They parse source through the formal syntax-output path, validate documentation
metadata, and either render docs or compile doctest snippets.

The main flow is:

1. Parse command-local arguments.
2. Read and parse source modules as syntax output.
3. Validate doc links and fenced code blocks.
4. For `doc`, optionally validate missing docs and render output.
5. For `doctest`, compile Terlan fenced snippets.

Important invariants:

- Parse errors are emitted before doc validation.
- Broken links and malformed fences prevent output.
- `doc --check` validates only and does not write output.
- `doc std --check` validates documentation examples for the release-owned
  standard-library source tree.
- `doc std` generates the standard-library reference site through the same
  public command path as user source documentation.
- `doc` writes one HTML page per module plus an aggregate static entry point at
  `<out-dir>/index.html`.
- `doc --format markdown` writes one Markdown page per module for users who
  explicitly want source-readable output.
- `doc --format json` writes one `terlan-doc-module-v1` JSON model per module
  plus an aggregate `terlan-doc-project-v1` model at `<out-dir>/model.json`.
- `doc --format html` is accepted explicitly and produces the same HTML shape
  as the default `doc` invocation.
- Documentation rendering includes public APIs only. Private
  declaration docs are preserved by syntax output but not emitted.
- `doctest` accepts exactly one source file path.
- `@example` blocks can contain REPL-style prompt examples using `>` input
  lines and following expected-output lines. `@example ignore`,
  `@example error`, and `@example target <name>` control whether examples are
  skipped, expected to fail, or checked only for a matching target profile.
- `doc --check` executes runnable prompt examples through the non-interactive
  REPL helper and compares exact output lines.

## Lifecycle

`main.rs` creates `CliCommand` and `CliState`, then transfers ownership to
these command entry points. Each invocation performs one synchronous validation
or render pass and owns no persistent process state.

## Scheduling And Ordering

- Argument validation happens before filesystem work.
- Output directories are created only for non-check documentation generation.
- Link and fence validation happen before missing-doc validation.
- Doctest compilation runs only after doc metadata validation succeeds.

## Data Structures

- `CliCommand`: carries command-local path and flags.
- `CliState`: carries output directory, incremental mode, diagnostic format,
  and documentation format.
- Syntax module output: formal parser output used as the documentation source.

## Integration Points

- `main.rs`: routes `doc` and `doctest` commands.
- `terlan_syntax`: parses modules into syntax output.
- `render`: renders Markdown and HTML documentation from syntax-output
  declarations, plus the compiler-owned JSON documentation model.
- `validation`: validates links/fences, checks missing docs, and compiles
  doctest snippets through formal phases before CoreIR-gated backend emission.
  It also owns REPL prompt example extraction and validation for the newer
  doc-check path.

## Edge Cases

- Missing or extra path arguments return exit code `2`.
- Unsupported `doc` flags return exit code `2`.
- Parse, validation, render-write, and doctest compile failures return exit code
  `1`.

## Destruction And Cleanup

The commands open no long-lived resources. Source strings, syntax outputs, and
rendered documentation buffers are dropped when the command returns.

## Types And Interfaces

`CliCommand`
: Command-local input container created by the top-level parser.

`CliState`
: Global CLI state used for diagnostics and documentation output settings.

Syntax module output
: Formal syntax output consumed by validation and rendering helpers.

## Testing Notes

- Existing focused doc and doctest tests still live in the large `main.rs` test
  module while test extraction is pending.
