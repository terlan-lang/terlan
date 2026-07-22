# Lint Command Tests

This directory owns test fixtures for `terlc lint`.

## Responsibilities

- Exercise lint command behavior through focused Rust tests.
- Keep rule families grouped by feature area.
- Prove diagnostics stay stable as lint rules grow.

## Invariants

- Test fixtures should assert diagnostic intent, not incidental formatting.
- New rule families need at least one positive and one negative test path.
- Generated-source behavior must stay explicit.

## Testing Notes

- Subdirectories group rule-specific regression tests.
