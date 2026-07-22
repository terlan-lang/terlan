# Direct JavaScript AST Internals

This directory owns focused CoreIR-to-Oxc AST lowering helpers used by the
direct JavaScript emitter. The parent module owns function and expression
dispatch; helpers here own expression families large enough to require an
independent lowering boundary.

## Responsibilities

- Lower ordered list-comprehension generators to JavaScript collection calls.
- Preserve generator order, guards, and nested flattening semantics.
- Return `None` when a pattern or expression cannot be represented faithfully.

## Integration Points

- `commands::emit_js::direct_ast`: supplies CoreIR expressions and Oxc builders.
- Oxc AST/codegen: receives arena-owned expressions without leaking Oxc types
  into CoreIR.

## Testing Notes

- Direct-AST tests cover single and nested generators, guards, and fallback.
- New comprehension pattern forms require success and rejection regressions.
