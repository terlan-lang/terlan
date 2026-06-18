# Terlan LSP Source Internals

This directory owns the Rust implementation of the Terlan LSP server. It is a
single-source module today and should be split when protocol routing,
diagnostic conversion, or document storage grows further.

## Responsibilities

- Store open document snapshots.
- Convert byte spans to LSP UTF-16 ranges.
- Run parse, resolve, and typecheck passes for editor diagnostics.
- Implement LSP lifecycle and document notification handlers.

## Public Surface

- `main`: starts the LSP server on standard input/output.
- `Backend`: `tower_lsp::LanguageServer` implementation.
- `OpenDocuments`: internal document store.

## Core Model

The source module keeps editor state shallow and delegates compiler work to
existing crates. It does not create a separate incremental compiler model.

The main flow is:

1. Open or change a document.
2. Parse, resolve, and type-check its current text.
3. Store the newest snapshot.
4. Publish diagnostics to the editor client.

Important invariants:

- UTF-16 LSP positions are derived from source text on demand.
- Diagnostics are published for the latest known text version.
- Compiler crates remain the source of language truth.

## Integration Points

- `terlan_syntax::parse_module_as_syntax_output`: parser entry point.
- `terlan_hir::resolve_syntax_module_output_with_interfaces`: resolver entry
  point.
- `terlan_typeck::type_check_syntax_module_output`: type checker entry point.
- `tower_lsp`: LSP server trait and protocol types.

## Edge Cases

- Byte offsets past the end of text produce no position.
- Parser failures still update the document snapshot.
- Interface-loading failures fall back to an empty interface map.

## Types And Interfaces

`OpenDocument`
: Snapshot of one editor document and its diagnostics.

`OpenDocuments`
: Shared map from URI to open document snapshot.

## Testing Notes

- Keep tests adjacent to this source module when it is split.
- Focus tests on span conversion, document lifecycle, and diagnostic mapping.
- End-to-end editor behavior should remain a separate integration concern.
