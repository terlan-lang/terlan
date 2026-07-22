# VM Memory Internals

This directory owns checkpoint and shared-allocation accounting helpers for VM
process memory. The parent memory accountant owns limits; these modules preserve
ownership and recovery invariants across subsystem boundaries.

## Responsibilities

- Snapshot and restore process memory accounting deterministically.
- Track shared allocations without double charging owners.
- Reject stale ownership and limit violations with typed outcomes.

## Core Model

Every charged allocation has an explicit owner or shared ownership record.
Checkpoint restore validates identity and totals before replacing live state;
partial restore is forbidden.

## Integration Points

- `runtime::vm::memory`: owns aggregate limits and pressure decisions.
- supervision and persistent actors consume checkpoint accounting.

## Testing Notes

- Parent memory and supervision pressure suites cover restore and cleanup.
- Changes require leak, stale-owner, overflow, and rollback regressions.
