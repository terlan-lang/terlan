# Migrate Command Internals

This directory owns explicit, source-preserving migrations exposed by
`terlc migrate`.

## Responsibilities

- Produce dry-run plans before any source write.
- Apply only mechanically safe rewrites with stable migration identifiers.
- Reject ambiguous candidates without partially modifying files.

## Testing Notes

Migration tests cover dry-run, write, idempotence, JSON output, and safe reject
behavior.
