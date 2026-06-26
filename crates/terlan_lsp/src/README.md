# Terlan LSP Source Internals

This directory owns the Rust implementation of the Terlan LSP server. The
server entry point stays in `lib.rs`; document storage and source-position
conversion live in `document.rs` so protocol routing can grow without turning
the server entry module back into a mixed-responsibility file.

## Responsibilities

- Store open document snapshots.
- Convert byte spans to LSP UTF-16 ranges.
- Run parse, resolve, and typecheck passes for editor diagnostics.
- Accept Terlan template language IDs without parsing them as source modules
  and validate template structure through `terlan_html`.
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
2. Classify the document by LSP language id.
3. Parse, resolve, and type-check source/interface text. Template documents
   skip source-module parsing and run target-aware structure validation.
4. Store the newest snapshot.
5. Publish diagnostics to the editor client.

Important invariants:

- UTF-16 LSP positions are derived from source text on demand.
- Diagnostics are published for the latest known text version.
- Compiler crates remain the source of language truth.
- Template files must not receive bogus Terlan module parse errors simply
  because an editor attaches them to the shared LSP.
- Template diagnostics are structure-oriented until interpolation typechecking
  and precise per-target spans are exposed through the template crate.

## Integration Points

- `terlan_syntax::parse_module_as_syntax_output`: parser entry point.
- `terlan_hir::resolve_syntax_module_output_with_interfaces`: resolver entry
  point.
- `terlan_typeck::type_check_syntax_module_output`: type checker entry point.
- `tower_lsp`: LSP server trait and protocol types.

## File Layout

- `lib.rs`: server construction, LSP request handlers, diagnostic conversion,
  document symbols, and same-document definition lookup.
- `document.rs`: open-document storage, document classification, compiler-path
  parsing/typechecking, template validation dispatch, and UTF-16 range
  conversion.
- `lib_test.rs`: LSP behavior tests.
- `main.rs`: standalone crate-local binary entry point.

## Edge Cases

- Byte offsets past the end of text produce no position.
- Parser failures still update the document snapshot.
- Interface-loading failures fall back to an empty interface map.

## Types And Interfaces

`OpenDocument`
: Snapshot of one editor document, its language id, category, and diagnostics.

`OpenDocuments`
: Shared map from URI to open document snapshot.

## Testing Notes

- Keep LSP tests adjacent in `lib_test.rs`.
- Focus tests on span conversion, document lifecycle, and diagnostic mapping.
- End-to-end editor behavior should remain a separate integration concern.
