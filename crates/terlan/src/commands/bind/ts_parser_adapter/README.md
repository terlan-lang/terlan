# TypeScript Parser Adapter Internals

This directory owns focused Oxc-to-Terlan declaration conversions that are too
large for the parent adapter module.

## Responsibilities

- Convert supported TypeScript literal types into deterministic binding plans.
- Preserve unsupported-shape diagnostics instead of approximating declarations.
- Keep parsing policy separate from generated source emission.

## Testing Notes

Exercise additions through the `ts_parser_adapter` tests and pinned `.d.ts`
namespace-generation gates.
