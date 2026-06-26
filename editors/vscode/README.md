# Terlan VS Code Extension

This package owns the local VS Code integration for Terlan source and template
files. It is intentionally small: file associations, basic syntax highlighting,
language configuration, and `terlc lsp --stdio` startup.

## Responsibilities

- Associate `.terl`, `.terli`, and `.terl.*` template files with Terlan
  language ids.
- Treat `.terl.html` templates as HTML-backed editor documents while keeping
  Terlan `${...}` expression highlighting and component links back to Terlan
  template declarations.
- Provide editor comment, bracket, and auto-closing behavior.
- Provide conservative TextMate highlighting until the Tree-sitter grammar is
  promoted.
- Package the shared Terlan SVG icon and fixed-size PNG variants for editor
  and marketplace surfaces.
- Start the Terlan language server through the compiler-owned `terlc lsp`
  command.

## Public Surface

- `package.json`: VS Code extension manifest.
- `icons/`: Terlan file icon theme assets copied from `editors/shared`.
- `language-configuration.json`: comments, brackets, and pairs.
- `syntaxes/terlan.tmLanguage.json`: conservative source highlighting.
- `syntaxes/terlan-template-html.tmLanguage.json`: HTML-backed template
  highlighting with Terlan expression islands.
- `src/client_config.js`: pure LSP command and document-selector
  configuration helpers.
- `src/extension.js`: extension activation and LSP startup.
- `src/template_links.js`: pure template tag/declaration link helpers.
- `test/client_config_test.js`: Node smoke tests for LSP startup
  configuration without loading VS Code.
- `test/manifest_test.js`: Node manifest tests for contributed language ids,
  activation events, grammar files, language configuration, and runtime
  dependencies.
- `test/package_smoke_test.js`: package-surface tests that verify runtime
  extension files are included and test/build artifacts are excluded.
- `test/pack_dry_run_test.js`: npm dry-run archive tests that verify the actual
  package file list before release.
- `test/diagnostics_smoke_test.js`: dependency-free diagnostics smoke tests
  that verify diagnostics flow through the compiler-owned LSP client instead
  of a duplicate extension-side checker.
- `test/textmate_bridge_test.js`: dependency-free bridge tests that compare
  temporary TextMate keyword/interpolation coverage against Tree-sitter
  highlight queries.

## Integration Points

- `terlc lsp --stdio`: compiler-owned language-server command.
- `vscode-languageclient`: VS Code LSP client package used at extension runtime.

## Language Server Deployment

The 0.0.5 editor deployment model is one compiler binary plus one editor
package. Users install `terlc`, install or load this VS Code extension, and the
extension starts the language server with:

```text
terlc lsp --stdio
```

The extension does not ship a separate language-server binary, does not start a
background daemon, and does not duplicate compiler checks in JavaScript. It
spawns `terlc` over standard input/output through `vscode-languageclient`, with
the working directory set to the first VS Code workspace folder when one is
available.

Advanced users may override `terlan.lsp.command` or `terlan.lsp.args` when
testing a locally built compiler, but the default release path should remain
`terlc lsp --stdio`.

## Testing Notes

- Manifest JSON must parse before release.
- `npm test` runs the dependency-free client configuration, manifest,
  package-surface, and diagnostics smoke tests.
- `npm run check` validates extension JavaScript syntax without packaging the
  extension.
- The package smoke validates `package.json.files` so packaging remains
  explicit without writing or committing a generated archive.
- `make vscode-extension-check` validates the actual `npm pack --dry-run`
  archive payload after running dependency-free extension tests.
- Extension packaging should not commit generated `.vsix` files unless that
  becomes the selected release artifact.
- Tree-sitter grammar and highlight tests should replace or generate the
  TextMate grammar when the grammar package is ready.
- Until that replacement lands, the TextMate bridge is checked against the
  Tree-sitter highlight query for core keyword and interpolation coverage.
