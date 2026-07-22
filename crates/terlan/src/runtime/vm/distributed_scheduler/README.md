# Distributed Scheduler Internals

This directory owns VM distributed-scheduler data structures and policy checks.

## Responsibilities

- Model placement, recovery, migration, and fault policy decisions.
- Keep distributed scheduling mechanics visible to VM tests.
- Avoid embedding network transport or consensus algorithms in scheduler state.

## Public Surface

- `mod.rs`: scheduler-facing types and coordination entry points.
- `fault.rs`: fault classification and recovery policy support.

## Invariants

- Scheduler decisions must be deterministic for the same inputs.
- Transport and storage adapters stay outside this module.
- Fault handling must surface typed VM diagnostics instead of panics.

## Testing Notes

- `distributed_scheduler_test/` owns focused placement, migration, recovery,
  and fault tests.
