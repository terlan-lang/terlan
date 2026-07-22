# SQL Forms Internals

This directory owns typecheck support for SQL forms. The implementation is
centered on typed SQL macro/query shapes. Its most important boundary is that
SQL parsing and validation should use maintained Rust crates where possible
instead of hand-rolled SQL logic.

## Responsibilities

- Recognize SQL-related syntax forms.
- Convert accepted SQL forms into typed core expressions.
- Surface diagnostics for unsupported SQL usage.

## Public Surface

- `mod.rs`: SQL form typecheck entry points.
- `validation.rs`: maintained `sqlparser` PostgreSQL syntax boundary.
- `classification.rs`: query-kind and cardinality inference over the parsed
  SQL AST.
- `projection.rs`: AST-owned output-name extraction and duplicate-name
  validation for SQL `SELECT` and `RETURNING` forms.
- `compiler/syntax/sql_regions.rs`: shared opaque-region and interpolation
  cursor logic used by both syntax extraction and typecheck binding; it does
  not classify statements, derive projections, or validate SQL syntax.

## Core Model

SQL forms are compiler-visible data access expressions that can later feed
database adapters and generated clients.

The main flow is:

1. Inspect syntax calls or macro forms.
2. Bind interpolation islands to PostgreSQL positional parameters.
3. Parse exactly one statement through the maintained PostgreSQL parser.
4. Derive query kind and conservative cardinality from AST nodes.
5. Derive projection field metadata from AST nodes when every output name is
   statically known, rejecting duplicate output names.
6. Produce a typed expression for downstream lowering.

Important invariants:

- SQL forms must stay explicit in source.
- Validation should be delegated to proven parser crates when available.
- Unsupported SQL must not silently lower to runtime strings.
- User-authored `$N` placeholders are rejected; every bound parameter must
  originate from a Terlan `${expression}` outside SQL quoted/comment regions.
- Projection extraction consumes parser-owned AST metadata; it is not an authoritative SQL
  parser or a substitute for live-schema validation.

## Integration Points

- `terlan_typeck`: owns typed conversion from syntax to core forms.
- Future database adapters: consume typed SQL metadata.

## Testing Notes

- Add tests for query parsing, parameter typing, and invalid SQL diagnostics.
