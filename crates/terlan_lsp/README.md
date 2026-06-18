# Terlan LSP Crate Internals

This crate owns the Language Server Protocol entry point for Terlan editor
integration. It is intentionally thin: it adapts editor events to parser,
interface loading, and type-checking services owned by the compiler crates.

## Responsibilities

- Start the Terlan LSP server process.
- Track open editor documents and publish diagnostics.
- Convert Terlan parser/type diagnostics into LSP ranges and messages.
- Keep editor protocol code separate from parser, HIR, and typechecker logic.

## Public Surface

- `terlan_lsp::main`: executable entry point for the LSP server.
- LSP initialize, open, change, close, and diagnostic handling through
  `tower_lsp`.

## Core Model

The LSP crate stores open document snapshots, parses each changed document,
loads nearby interfaces, resolves the syntax module, and publishes diagnostics
back to the editor client.

The main flow is:

1. Receive a document event from the LSP client.
2. Parse and type-check the latest document text.
3. Convert compiler diagnostics into LSP diagnostics.
4. Publish diagnostics for the document URI.

Important invariants:

- LSP code must not own language semantics.
- Source spans are converted to UTF-16 LSP positions at the boundary.
- Interface loading remains best-effort so editor diagnostics stay responsive.

## Integration Points

- `terlan_syntax`: parses source into syntax output.
- `terlan_hir`: loads interfaces and resolves module references.
- `terlan_typeck`: produces type diagnostics.
- `tower_lsp`: owns JSON-RPC and LSP transport behavior.

## Edge Cases

- Invalid file URLs produce empty interface context instead of panicking.
- Serialization/parser artifact failures map to zero-width diagnostics.
- CRLF line endings are normalized during range conversion.

## Types And Interfaces

`Backend`
: LSP server implementation that handles editor events.

`OpenDocuments`
: Shared open-document store used by LSP event handlers.

## Testing Notes

- Source-position conversion and document-state behavior should be covered in
  adjacent Rust tests.
- LSP tests should avoid depending on a real editor process.
- Parser/typechecker behavior belongs in the owning compiler crates.
