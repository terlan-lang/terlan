# Terlan LSP Crate Internals

This crate owns the Language Server Protocol entry point for Terlan editor
integration. It is intentionally thin: it adapts editor events to parser,
interface loading, and type-checking services owned by the compiler crates.

## Responsibilities

- Start the Terlan LSP server process.
- Track open editor documents and publish diagnostics.
- Convert Terlan parser/type diagnostics into LSP ranges and messages.
- Accept editor-registered Terlan template language IDs without reporting
  source-module parse errors for template bodies, and validate template
  structure through `terlan_html`.
- Keep editor protocol code separate from parser, HIR, and typechecker logic.

## Public Surface

- `terlan_lsp::main`: executable entry point for the LSP server.
- LSP initialize, open, change, close, and diagnostic handling through
  `tower_lsp`.

## Core Model

The LSP crate stores open document snapshots, classifies each document by LSP
language id, parses source/interface documents, loads nearby interfaces,
resolves syntax modules, and publishes diagnostics back to the editor client.
Template documents skip source-module parsing and publish target-aware
structure diagnostics where the shared template validators support them.

The main flow is:

1. Receive a document event from the LSP client.
2. Classify the document by language id.
3. Parse and type-check source/interface text, or validate template text through
   the shared template validators without source-module parsing.
4. Convert compiler diagnostics into LSP diagnostics.
5. Publish diagnostics for the document URI.

Important invariants:

- LSP code must not own language semantics.
- Source spans are converted to UTF-16 LSP positions at the boundary.
- Interface loading remains best-effort so editor diagnostics stay responsive.
- Template support must reuse the shared LSP attachment point without faking
  template bodies as ordinary Terlan modules.

## Integration Points

- `terlan_syntax`: parses source into syntax output.
- `terlan_hir`: loads interfaces and resolves module references.
- `terlan_typeck`: produces type diagnostics.
- `tower_lsp`: owns JSON-RPC and LSP transport behavior.

## Deployment Contract

The release-facing language-server deployment path is `terlc lsp --stdio`.
Editor packages should spawn that command from the installed compiler instead
of bundling a second language-server executable or running a persistent daemon.

The standalone `terlan-lsp --stdio` binary remains useful for crate-local
development and direct protocol testing, but it is not the default user-facing
deployment artifact. Keeping the LSP inside `terlc` ensures editor diagnostics
use the same parser, resolver, typechecker, std summaries, and release version
as command-line builds.

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
