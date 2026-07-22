# VM Supervision Internals

This directory owns focused supervision policies and adversarial fixtures that
do not belong in the parent restart-tree implementation.

## Responsibilities

- Validate restart cleanup under VM memory pressure.
- Preserve child ownership across restart and shutdown ordering.
- Prove failed recovery cannot retain stale process resources.

## Integration Points

- `runtime::vm::supervision`: owns child specifications and restart strategy.
- `runtime::vm::memory`: supplies process memory ownership and pressure results.

## Testing Notes

- `memory_pressure_test.rs` covers restart, cleanup, and ownership failures.
- Every new restart path needs both successful recovery and exhausted-budget
  coverage.
