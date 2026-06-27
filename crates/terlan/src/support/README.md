# CLI Support Helpers

This module owns shared process-level support used by command modules. It is
kept outside `main.rs` so the entry file can focus on global argument parsing
and verb routing.

## Responsibilities

- Render compiler diagnostics in text or JSON form.
- Resolve diagnostic color behavior.
- Read command input files with user-facing errors.
- Write generated outputs with incremental no-op behavior.
- Convert Terlan module names into backend output stems.

## Public Surface

- `diagnostic_color`: resolves color behavior from diagnostic format.
- `emit_diagnostic`: writes text or JSON diagnostics.
- `read_file`: reads a source file as UTF-8 text.
- `write_if_changed_or_forced`: writes output bytes with incremental skipping.
- `erlang_output_stem`: converts dotted module names to lower-case Erlang file
  stems.

## Core Model

Support helpers may perform process IO, but they do not parse command-local
arguments and they do not decide command exit codes. Commands call these helpers
and remain responsible for presenting user-facing command behavior.

Important invariants:

- Diagnostic rendering stays centralized so parse, resolve, and type errors
  have one output shape.
- Incremental write semantics stay centralized so emit commands do not drift.
- This module should stay service-oriented; formal compile phases live in
  `formal_pipeline`.

## Integration Points

- `formal_pipeline`: emits parse, resolve, and type diagnostics.
- `commands::*`: read source files, write outputs, and emit command diagnostics.
- `main.rs`: uses only `diagnostic_color` during global flag parsing.

## Testing Notes

Current coverage is through CLI integration tests. Add module-local tests if
diagnostic formatting or incremental write semantics become more complex.
