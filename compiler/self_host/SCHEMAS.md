# Self-Hosted Compiler Schemas

The self-hosted compiler communicates with Rust bootstrap and backend
components only through versioned canonical documents. Private Rust syntax,
HIR, type-checker, Cranelift, and VM structures are not schema fields.

## `terlan.self-host.tokens.v1`

Contains ordered tokens with lexical kind, exact lexeme, and half-open UTF-8
byte plus one-based line/column spans. Comments and whitespace are trivia and
are not emitted. Invalid source characters produce lexical diagnostics rather
than replacement tokens.

## `terlan.self-host.interface.v1`

Contains module identity, imports in source order, and public declaration
kind/name rows. It is intentionally narrower than the future typed public API
manifest.

## `terlan.self-host.source-evidence.v1`

Binds the source name and SHA-256 digest to token and interface documents,
acceptance status, and typed lexical/interface diagnostics. This is the first
differential envelope consumed by stage-0 and stage-1 comparisons.

## Evolution rules

- Schema names and versions are explicit fields.
- Existing field meaning cannot change within one version.
- Additive optional evidence requires a documented default.
- Removed, renamed, reordered-semantic, or reinterpreted fields require a new
  schema version.
- Reports from different source revisions cannot form one bootstrap candidate.
- Native object bytes are not a substitute for canonical semantic evidence.
