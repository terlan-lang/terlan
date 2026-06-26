# Terlan IntelliJ-Family Integration Internals

This directory owns the minimal IntelliJ-family editor integration for Terlan.
The implementation is centered on file type registration and startup for the
compiler-owned language server. Its most important boundary is that the plugin
does not embed a second parser, typechecker, compiler, or daemon.

## Responsibilities

- Register Terlan source, interface, and template suffixes.
- Start Terlan's language server through `terlc lsp --stdio`.
- Prefer `terlan.toml` as the project root marker, with `.git` fallback.
- Reuse the canonical shared Terlan SVG file icon and fixed-size PNG variants.

## Public Surface

- `src/main/resources/META-INF/plugin.xml`: JetBrains plugin metadata.
- `src/main/resources/icons/`: Terlan file icon assets copied from
  `editors/shared`.
- `src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt`: File type contract.
- `src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt`: LSP
  startup contract.
- `test/package_smoke_test.js`: Dependency-free package contract checks.

## Core Model

The IntelliJ package is editor glue over the installed compiler. JetBrains IDEs
own the UI process, and `terlc` owns language semantics.

The main flow is:

1. IntelliJ recognizes a Terlan file suffix.
2. The plugin associates the file with the Terlan file type and icon.
3. The plugin starts `terlc lsp --stdio` for language diagnostics and symbols.

Important invariants:

- `terlc lsp --stdio` is the only default language-server command.
- The plugin never shells out to a separate `terlan-lsp` binary by default.
- File icon metadata points at the shared canonical Terlan editor icon.
- Packaged PNG icon variants remain byte-equivalent to the shared variants.

## Integration Points

- `terlc lsp --stdio`: Compiler-owned LSP process.
- JetBrains LSP API: Preferred plugin API where available.
- LSP4IJ: Possible fallback adapter if the official API is unavailable for a
  target IDE.

## Edge Cases

- Some IntelliJ-family IDEs may not expose the official LSP API. In that case,
  the plugin should either use a thin LSP4IJ adapter or document local setup.
- Marketplace publishing is not required for 0.0.5; local package validation is
  sufficient.
- Generated plugin archives must not be committed.

## Types And Interfaces

`TerlanFileTypes`
: Declares Terlan file suffixes, display names, and icon path metadata.

`TerlanLspServerDescriptor`
: Declares the compiler-owned LSP command and root markers.

## Testing Notes

- `make intellij-editor-check` runs dependency-free package smoke checks.
- Smoke tests validate file suffixes, LSP command, root markers, shared icon
  metadata, packaged PNG variants, and absence of generated plugin artifacts.
