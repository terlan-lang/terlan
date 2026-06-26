# Terlan Shared Editor Assets Internals

This directory owns editor assets that must stay identical across Terlan editor
packages. The implementation is centered on a canonical SVG file icon,
package-ready PNG variants, and dependency-free smoke checks. Its most
important boundary is that editor packages may reference or package these
assets, but they must not invent a different Terlan visual identity.

## Responsibilities

- Own the canonical Terlan source/interface/template file icon.
- Own generated PNG variants for editor ecosystems that require fixed-size
  raster assets.
- Provide smoke checks for icon existence, SVG shape, PNG dimensions, shared
  file suffixes, and compiler-owned LSP command metadata.
- Keep shared editor assets independent from any one editor runtime.
- Prevent generated editor package artifacts from becoming the source of truth.

## Public Surface

- `icons/terlan-file.svg`: Canonical Terlan file icon asset.
- `icons/png/terlan-file-*.png`: Generated package-ready icon variants.
- `test/icon_smoke_test.js`: Dependency-free shared icon contract check.
- `test/editor_contract_test.js`: Dependency-free cross-editor suffix and LSP
  contract check.

## Core Model

The shared editor asset model is intentionally small. A checked-in SVG is the
source of truth, generated PNG variants are derived package assets, and editor
packages either reference or copy the shared assets during package production.

The main flow is:

1. Keep the canonical icon under `editors/shared/icons`.
2. Editor package smoke tests verify their metadata points at the canonical
   asset or a byte-equivalent packaged copy.
3. Release packaging can generate editor-specific copies without changing the
   canonical source.

Important invariants:

- The canonical icon remains an SVG source file.
- PNG variants remain generated from the shared icon design and must preserve
  expected square dimensions.
- Editor packages do not define unrelated Terlan file icons.
- Editor packages register the same `.terl`, `.terli`, and `.terl.*` template
  suffix family.
- Editor packages use `terlc lsp --stdio` as their default compiler-owned
  language-server command.
- Generated package archives are not committed as shared editor assets.

## Integration Points

- `editors/vscode`: VS Code file icon metadata and package smoke tests.
- `editors/intellij`: JetBrains file type icon metadata and package smoke
  tests.
- `editors/neovim` / `editors/emacs`: Optional icon mapping metadata for users
  with icon-capable UI plugins.

## Edge Cases

- Editors that cannot consume SVG directly may use generated copies, but the
  generation source remains `icons/terlan-file.svg`.
- Neovim and Emacs may not expose first-party file icon APIs; their packages
  should document optional integration rather than inventing required runtime
  behavior.
- Missing icon metadata should fail package smoke tests once an editor package
  declares icon support.

## Types And Interfaces

`terlan-file.svg`
: Canonical editor file icon for Terlan source, interface, and template files.

`terlan-file-*.png`
: Package-ready raster variants for editors or marketplaces that require fixed
  icon sizes.

## Testing Notes

- `make shared-editor-icon-check` runs the icon smoke test.
- `make shared-editor-contract-check` runs the cross-editor suffix and LSP
  contract smoke test.
- The icon smoke test validates the SVG source and 16, 24, 32, 64, and 128
  pixel PNG variants.
- The editor contract test validates VS Code, Tree-sitter, Neovim, Emacs, and
  IntelliJ metadata against one shared suffix/LSP model.
- Add focused editor package smoke checks whenever a package starts consuming
  the shared icon.
