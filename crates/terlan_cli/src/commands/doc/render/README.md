# Documentation Render Internals

This directory owns concrete renderers for `terlc doc`. The implementation
turns validated documentation models into target output formats without
re-reading source files or changing documentation semantics.

## Responsibilities

- Render documentation pages into stable output formats.
- Keep format-specific escaping and layout details isolated from command logic.
- Preserve validated links, examples, signatures, and module metadata.
- Avoid partial output when a renderer reports an error.

## Public Surface

- `html`: HTML rendering for documentation output.

## Core Model

The command-level documentation pipeline validates source documentation before
control reaches this directory. Renderer modules receive already-normalized
documentation data and own only presentation transforms.

The main flow is:

1. Receive normalized documentation structures from `commands::doc`.
2. Convert those structures into deterministic format-specific text.
3. Return rendered bytes or a diagnostic to the command layer.

Important invariants:

- Renderers do not perform compiler parsing or typechecking.
- Renderers must not silently change validated example text.
- Format errors must be reported before output is committed.

## Integration Points

- `crates/terlan_cli/src/commands/doc`: validates documentation inputs.
- `crates/terlan_html`: sanitizes and models HTML-oriented content.
- `terlc doc`: exposes renderer output to users.

## Edge Cases

- Empty documentation still renders a stable module shell when requested.
- Unsafe HTML must be sanitized or rejected before output.
- Missing examples are allowed, but malformed examples stay diagnostics.

## Types And Interfaces

`html`
: HTML renderer module for generated documentation pages.

## Testing Notes

- Renderer tests live adjacent to the doc command modules.
- Changes to escaping, page shape, or example preservation need focused tests.
- End-to-end docs behavior is covered through `terlc doc` command tests.
