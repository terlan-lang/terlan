# Syntax Formatter Internals

This directory owns source formatting helpers. The implementation is centered on
canonical Terlan syntax output. Its most important boundary is that formatting
must preserve semantics while normalizing style.

## Responsibilities

- Format parsed Terlan syntax into canonical source text.
- Keep formatting rules deterministic.
- Avoid changing compiler semantics during formatting.

## Public Surface

- `mod.rs`: formatter entry points used by CLI formatting commands.

## Core Model

The formatter consumes syntax structures and writes normalized source text.

The main flow is:

1. Parse source into syntax output.
2. Render declarations and expressions with canonical spacing.
3. Return formatted text or syntax diagnostics.

Important invariants:

- Formatting must be idempotent.
- Formatter output must parse back into the same syntax contract.
- Unsupported syntax must fail without partial rewrites.

## Integration Points

- `terlan_syntax`: provides parser output.
- `terlan_cli`: invokes formatting behavior.

## Testing Notes

- Add fixture tests for every formatting rule that changes source layout.
