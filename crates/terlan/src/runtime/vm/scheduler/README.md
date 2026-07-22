# VM Scheduler Internals

This directory owns focused scheduler support built around the VM process and
run-queue model.

## Responsibilities

- Record deterministic scheduler telemetry without changing scheduling policy.
- Preserve process identity and reduction accounting across observations.
- Keep metrics collection bounded under adversarial process counts.

## Testing Notes

Run scheduler, fairness, telemetry, and strict VM coverage gates.
