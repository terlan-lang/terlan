# Lint Command Internals

This directory owns `terlc lint`, the opinionated style and correctness lint
surface for Terlan projects.

## Responsibilities

- Parse one or more lint inputs in a single process.
- Run lint rules over Terlan source files.
- Emit stable lint diagnostics for CLI and editor consumers.
- Keep formatting policy separate from lint-only readability guidance.

## Public Surface

- `mod.rs`: command entry point and rule orchestration.
- `diagnostic.rs`: lint diagnostic data model.
- `paths.rs`: source path discovery and filtering.
- `rules.rs`: rule registration.

## Invariants

- Lints must not rewrite files; fixes belong to formatter or codemods.
- Rule diagnostics must be deterministic across platforms.
- Overlapping input roots must lint each source file exactly once.
- Repeated `--only` selectors share source discovery and compatible parser
  work in one process.
- Generated files and test fixtures must be classified explicitly.

## Testing Notes

- `lint_test.rs` and nested `lint_test/` modules own command and rule fixtures.
