# Erlang Syntax Call Emission Internals

This directory owns call-shape helpers for syntax-level Erlang emission.

## Responsibilities

- Lower Terlan call syntax into Erlang call forms.
- Keep receiver, remote, constructor, and intrinsic call details separated from
  broader expression emission.
- Preserve deterministic Erlang text for stable tests.

## Public Surface

- Module-local helpers consumed by `backends::erlang::emit::syntax`.

## Integration Points

- `compiler::syntax`: supplies syntax-output call structures.
- `backends::erlang::emit::syntax`: assembles complete Erlang expressions.

## Testing Notes

- Add syntax emission tests for each new call shape.
- Keep generated Erlang fixtures stable and explicit.
