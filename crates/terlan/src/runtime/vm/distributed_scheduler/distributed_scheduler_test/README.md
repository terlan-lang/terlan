# Distributed Scheduler Tests

This directory owns tests for VM distributed-scheduler behavior.

## Responsibilities

- Exercise placement decisions.
- Exercise migration decisions.
- Exercise recovery and fault handling paths.

## Invariants

- Tests should keep network effects simulated and deterministic.
- Scheduler policy changes need tests for both accepted and rejected decisions.
- Fault handling tests must assert typed diagnostics where failures are expected.

## Testing Notes

- Submodules group placement, migration, recovery, and fault coverage.
