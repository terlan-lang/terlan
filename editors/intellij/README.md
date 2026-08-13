# Terlan IntelliJ-Family Integration Internals

This directory owns the minimal IntelliJ-family editor integration for Terlan.
The implementation is centered on file type registration and startup for the
compiler-owned language server. Its most important boundary is that the plugin
does not embed a second parser, typechecker, compiler, or daemon.

## Responsibilities

- Register Terlan source, interface, and template suffixes.
- Start Terlan's language server through `terlc lsp --stdio`.
- Prefer `terlan.toml` as the project root marker, with `.git` fallback.
- Reuse the canonical shared Terlan SVG file icon.

## Public Surface

- `src/main/resources/META-INF/plugin.xml`: JetBrains plugin metadata.
- `src/main/resources/icons/`: Terlan file icon assets copied from
  `editors/shared`.
- `src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt`: File type contract.
- `src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt`: LSP
  startup contract.
- `gradlew`: pinned Gradle 9 build entry point required by the current
  IntelliJ Platform Gradle plugin.
- `test/package_smoke_test.js`: Dependency-free package contract checks.

## Core Model

The IntelliJ package is editor glue over the installed compiler. JetBrains IDEs
own the UI process, and `terlc` owns language semantics.

The main flow is:

1. IntelliJ recognizes a Terlan file suffix.
2. The plugin associates the file with the Terlan file type and icon.
3. The registered JetBrains LSP provider starts `terlc lsp --stdio` once per
   project for diagnostics, navigation, completion, semantic tokens, and
   compiler-backed formatting.

Important invariants:

- `terlc lsp --stdio` is the only default language-server command.
- The plugin never shells out to a separate `terlan-lsp` binary by default.
- File icon metadata points at the shared canonical Terlan editor icon.
- The package contains no unused raster icon copies or generated build output.

## Integration Points

- `terlc lsp --stdio`: Compiler-owned LSP process.
- JetBrains LSP API: The supported plugin API and runtime integration.

## Edge Cases

- The 2024.3 baseline requires a commercial IntelliJ-platform IDE because its
  LSP API is supplied by the Ultimate platform module.
- Marketplace publishing is not required for 0.0.5; local package validation is
  sufficient.
- Generated plugin archives must not be committed.

## Types And Interfaces

`TerlanFileTypes`
: Declares Terlan file suffixes, display names, and icon path metadata.

`TerlanLspServerDescriptor`
: Declares the compiler-owned LSP command and root markers.

## Testing Notes

- `make intellij-editor-check` runs package smoke checks and builds the plugin
  against the pinned IntelliJ Ultimate 2024.3 platform.
- Smoke tests validate file suffixes, real LSP registration, command/root
  contracts, shared icon metadata, and package/build-input inventory.
