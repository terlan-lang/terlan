# Tree-Sitter Terlan

This directory owns the editor-facing Tree-sitter grammar scaffold for Terlan.
The compiler grammar remains the source of truth; this grammar exists for
syntax highlighting, editor navigation, and mixed template-region support.

## Responsibilities

- Parse representative Terlan source for editor tooling.
- Advertise `.terl`, `.terli`, and `.terl.*` template suffixes through
  Tree-sitter package metadata.
- Provide highlight queries for modules, imports, declarations, annotations,
  types, strings, comments, and interpolation islands.
- Provide injection queries so `${...}` template expression islands reuse
  Terlan highlighting inside `.terl.*` template files.
- Keep editor syntax support separate from compiler validation.
- Provide a path to generate or replace the temporary VS Code TextMate grammar.

## Public Surface

- `grammar.js`: Tree-sitter grammar scaffold.
- `queries/highlights.scm`: highlight query rules.
- `queries/injections.scm`: template expression-island injection rules.
- `test/corpus/basic.txt`: initial parser corpus.
- `test/package_smoke_test.js`: dependency-free package, script, corpus, and
  highlight query coverage smoke.
- `test/pack_dry_run_test.js`: npm dry-run archive validator for the actual
  publishable package file list.
- `package.json`: Tree-sitter package metadata and local commands.

## Integration Points

- `editors/vscode`: consumes generated highlighting or a generated TextMate
  bridge once grammar generation is part of the local toolchain.
- `terlc lsp --stdio`: remains responsible for diagnostics and semantic editor
  features.

## Testing Notes

- Run `npm install` only when preparing the editor package locally.
- Run `npm run check` to validate package metadata, Tree-sitter command wiring,
  grammar syntax, selected package files, required query/corpus inputs, corpus
  content, and highlight/injection query capture coverage.
- `make tree-sitter-package-check` also validates the actual
  `npm pack --dry-run` archive payload without publishing a package.
- Run `npm test` from this directory once `tree-sitter-cli` is installed to
  execute the full Tree-sitter corpus parser test.
- Do not commit generated parser artifacts unless they become selected release
  artifacts.
