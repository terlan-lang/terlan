# Terlan Emacs Integration

This package owns the minimal Emacs integration for Terlan source and template
files. It provides a major mode, suffix registration, and LSP client setup that
uses the installed compiler.

## Responsibilities

- Register `.terl`, `.terli`, and `.terl.*` template files.
- Provide a conservative `terlan-mode` for source editing.
- Prefer `terlan-ts-mode` when Emacs Tree-sitter support and the shared
  `terlan` grammar are available.
- Configure `eglot` and `lsp-mode` to start `terlc lsp --stdio`.
- Keep parsing, diagnostics, symbols, and typechecking inside `terlc`.

## Public Surface

- `terlan-mode.el`: Emacs major mode and LSP registration.
- `test/package_smoke_test.js`: dependency-free package contract checks.

## Integration Points

- `terlc lsp --stdio`: compiler-owned LSP command.
- `eglot`: built-in Emacs LSP client path.
- `lsp-mode`: optional community LSP client path.
- `tree-sitter-terlan`: optional shared parser package for highlighting.

## Testing Notes

- `make emacs-editor-check` runs the dependency-free package smoke.
- Smoke tests validate the LSP command, root markers, suffix registration,
  optional Tree-sitter remapping, and expected package files without launching
  Emacs.
