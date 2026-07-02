# Benchmark Internals

This directory owns Terlan benchmark probes that compare compiler/runtime lanes
without making benchmark results part of the public compiler API. The
implementation is centered on small typed benchmark reports that can be run from
the `terlan-benchmark` binary. Its most important boundary is that benchmarks
may observe runtime behavior, but they must not become required runtime
machinery.

## Responsibilities

- Provide reproducible benchmark scenarios for runtime and compiler migration
  work.
- Keep benchmark inputs small, explicit, and correct before measuring timing.
- Report lane capability status when a benchmark target is not implemented yet.
- Avoid hiding runtime gaps behind synthetic success results.

## Public Surface

- `main.rs`: command-line entry point for the benchmark binary.
- `http_runtime_lane.rs`: HTTP runtime lane capability and report helpers.
- `native_modules.rs`: native module benchmark metadata and fixtures.

## Core Model

Benchmark modules build typed scenario descriptions and convert them into
human-readable reports. A report can include measured timing when a lane is
implemented, or a stable unavailable status when the runtime path is not ready
to execute the scenario.

The main flow is:

1. Select a benchmark scenario from the benchmark binary.
2. Build the lane-specific runtime plan or capability status.
3. Render a report that can be compared across future runtime changes.

Important invariants:

- Benchmarks must distinguish unavailable runtime capability from failure.
- Benchmark scenarios must keep correctness assertions near the measured path.
- Benchmark code must not introduce production dependencies into compiler or VM
  execution paths.

## Integration Points

- `runtime::vm`: supplies VM execution capability status and future execution
  hooks.
- `runtime::native`: supplies Rust-owned native adapters used by benchmark
  scenarios.
- `commands::serve`: defines HTTP behavior that benchmark probes must not
  duplicate.

## Edge Cases

- A not-yet-implemented Terlan VM lane is reported as typed unavailable, not as
  a zero-duration success.
- HTTP benchmark probes must not hand-roll protocol parsing or server behavior.
- Native-boundary measurements must keep capability names and failure modes
  stable so future regressions are visible.

## Types And Interfaces

`HttpRuntimeLane`
: Identifies the runtime lane being described by an HTTP benchmark report.

`HttpRuntimeReport`
: Captures whether an HTTP benchmark lane is executable, unavailable, or failed
  with a stable diagnostic.

## Testing Notes

- `http_runtime_lane.rs` contains focused tests for typed unavailable VM lane
  reporting.
- Add focused tests when new benchmark report states are introduced.
- Keep large performance sweeps outside normal unit tests; release gates should
  validate shape and correctness, not depend on machine-specific timing.
