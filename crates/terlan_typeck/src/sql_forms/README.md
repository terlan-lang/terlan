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
- `projection.rs`: conservative projection extraction for simple SQL
  `SELECT` and `RETURNING` forms.
- `scanner.rs`: token-level SQL scanner helpers used by SQL form analysis.

## Core Model

SQL forms are compiler-visible data access expressions that can later feed
database adapters and generated clients.

The main flow is:

1. Inspect syntax calls or macro forms.
2. Validate the accepted SQL shape.
3. Extract conservative metadata, such as cardinality and simple projection
   field names, when the SQL shape is unambiguous.
4. Produce a typed expression for downstream lowering.

Important invariants:

- SQL forms must stay explicit in source.
- Validation should be delegated to proven parser crates when available.
- Unsupported SQL must not silently lower to runtime strings.
- Projection extraction is compatibility metadata, not an authoritative SQL
  parser.

## Integration Points

- `terlan_typeck`: owns typed conversion from syntax to core forms.
- Future database adapters: consume typed SQL metadata.

## Testing Notes

- Add tests for query parsing, parameter typing, and invalid SQL diagnostics.
