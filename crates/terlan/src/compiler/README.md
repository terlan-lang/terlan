# Compiler Internals

This directory owns Terlan front-end compilation and compiler-owned contracts.

## Responsibilities

- Parse Terlan source into syntax output.
- Resolve module and interface metadata.
- Typecheck modules and lower checked programs into CoreIR.
- Own compiler-level contracts such as API extraction and router syntax helpers.

## Public Surface

- `syntax`: lexer, parser, formatter, and syntax contracts.
- `hir`: module and interface resolution.
- `typeck`: type checking and CoreIR lowering.
- `api_contract`: typed API contract extraction.

## Integration Points

- `commands`: invokes compiler phases for CLI operations.
- `backends`: consumes checked compiler output.
- `validation`: checks target and release contracts over compiler artifacts.

## Testing Notes

- Keep tests adjacent to the compiler phase they validate.
- Add adversarial tests for ambiguous syntax, unresolved symbols, and target
  profile mismatches.
