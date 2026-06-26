# Terlan Neovim Integration

This package owns the minimal Neovim integration for Terlan source and template
files. It is intentionally thin: filetype detection, buffer-local setup, and
startup for the compiler-owned language server.

## Responsibilities

- Register `.terl`, `.terli`, and `.terl.*` template filetypes.
- Register those filetypes with the shared `terlan` Tree-sitter parser when
  Neovim exposes `vim.treesitter.language.register`.
- Start Terlan's language server through `terlc lsp --stdio`.
- Discover project roots from `terlan.toml` before falling back to `.git`.
- Keep parsing, diagnostics, symbols, and typechecking inside `terlc`.

## Public Surface

- `ftdetect/terlan.lua`: filetype detection for Terlan source and templates.
- `ftplugin/terlan.lua`: buffer-local setup hook.
- `lua/terlan_lsp.lua`: shared LSP command and startup helpers.
- `test/package_smoke_test.js`: dependency-free package contract checks.

## Integration Points

- `terlc lsp --stdio`: compiler-owned LSP command.
- `vim.lsp.start`: Neovim LSP client entry point.
- `vim.fs.root`: preferred project-root discovery API when available.
- `tree-sitter-terlan`: optional shared parser package for highlighting.

## Testing Notes

- `make neovim-editor-check` runs the dependency-free package smoke.
- Smoke tests validate the LSP command, root markers, suffix registration, and
  optional Tree-sitter registration without launching Neovim.
